use std::convert::Infallible;
use std::future::IntoFuture;
use std::sync::Arc;

use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use futures::Stream;
use serde::Deserialize;
use sqlx::SqlitePool;
use tokio::net::{TcpListener, UnixListener};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::config::DekuConfig;
use crate::container::DockerClient;
use crate::db::queries;
use crate::deploy::{DeployRequest, DeploySource};
use crate::events::EventSender;
use deku_core::types::{NewApp, Upstream};

pub mod auth;

#[derive(Clone)]
pub struct AppState {
    pub config: DekuConfig,
    pub pool: SqlitePool,
    pub events: EventSender,
    pub docker: DockerClient,
}

impl AppState {
    pub fn new(
        config: DekuConfig,
        pool: SqlitePool,
        events: EventSender,
        docker: DockerClient,
    ) -> Arc<Self> {
        Arc::new(Self {
            config,
            pool,
            events,
            docker,
        })
    }
}

pub type SharedState = Arc<AppState>;

fn build_router(state: SharedState) -> Router {
    Router::new()
        // Health
        .route("/healthz", get(health_check))
        // Apps
        .route("/api/apps", get(list_apps).post(create_app))
        .route("/api/apps/{name}", get(get_app).delete(delete_app))
        // Events
        .route("/api/events", get(list_events))
        .route("/api/events/stream", get(stream_events))
        // Domains
        .route(
            "/api/apps/{name}/domains",
            get(list_domains).post(add_domain),
        )
        .route(
            "/api/apps/{name}/domains/{domain}",
            delete(remove_domain),
        )
        // Ports
        .route("/api/apps/{name}/ports", get(list_ports).post(add_port))
        .route("/api/apps/{name}/ports/{id}", delete(remove_port))
        // Deployments
        .route("/api/apps/{name}/deployments", get(list_deployments))
        .route("/api/apps/{name}/deploy", post(trigger_deploy))
        .route("/api/apps/{name}/rollback", post(trigger_rollback))
        // Logs
        .route("/api/apps/{name}/logs", get(get_logs))
        // Config vars
        .route(
            "/api/apps/{name}/config",
            get(list_config).post(set_config),
        )
        .route("/api/apps/{name}/config/{key}", delete(unset_config))
        // Process scale
        .route("/api/apps/{name}/scale", get(get_scale).post(set_scale))
        // Routing table
        .route("/api/routing", get(list_routing))
        .route("/api/routing/{name}", post(update_routing))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

pub async fn serve(state: SharedState) -> anyhow::Result<()> {
    // Generate and persist the CLI token on startup
    if let Some(secret) = &state.config.auth_secret {
        match auth::generate_token(secret) {
            Ok(token) => {
                let token_path = state.config.data_dir.join("cli-token");
                std::fs::create_dir_all(&state.config.data_dir)?;
                std::fs::write(&token_path, &token)?;
                info!(
                    token_path = %token_path.display(),
                    "CLI auth token written"
                );
            }
            Err(e) => {
                tracing::warn!("failed to generate CLI auth token: {e}");
            }
        }
    }

    // Unix socket router — trusted local access, no auth required
    let unix_app = build_router(state.clone());

    // TCP router — requires valid JWT Bearer token
    let tcp_app = build_router(state.clone()).layer(middleware::from_fn_with_state(
        state.clone(),
        auth::require_auth,
    ));

    let tcp_addr = format!("0.0.0.0:{}", state.config.api_port);
    let tcp_listener = TcpListener::bind(&tcp_addr).await?;
    info!("API listening on {tcp_addr}");

    let sock_path = &state.config.socket_path;
    if sock_path.exists() {
        std::fs::remove_file(sock_path)?;
    }
    std::fs::create_dir_all(sock_path.parent().unwrap())?;
    let unix_listener = UnixListener::bind(sock_path)?;
    info!("API listening on {}", sock_path.display());

    tokio::try_join!(
        axum::serve(tcp_listener, tcp_app).into_future(),
        axum::serve(unix_listener, unix_app).into_future(),
    )?;

    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn internal_error(e: impl std::fmt::Display) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": e.to_string() })),
    )
}

fn not_found(msg: impl std::fmt::Display) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": msg.to_string() })),
    )
}

// ── Health ────────────────────────────────────────────────────────────────────

async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok", "service": "dekud" }))
}

// ── Apps ──────────────────────────────────────────────────────────────────────

async fn list_apps(State(state): State<SharedState>) -> impl IntoResponse {
    match queries::list_apps(&state.pool).await {
        Ok(apps) => (StatusCode::OK, Json(serde_json::json!(apps))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

async fn create_app(
    State(state): State<SharedState>,
    Json(body): Json<NewApp>,
) -> impl IntoResponse {
    match queries::create_app(&state.pool, &body).await {
        Ok(app) => {
            state.events.emit(
                Some(app.id.clone()),
                "app.created",
                Some(serde_json::json!({ "name": app.name })),
            );
            (StatusCode::CREATED, Json(serde_json::json!(app))).into_response()
        }
        Err(e) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn get_app(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> impl IntoResponse {
    match queries::get_app(&state.pool, &name).await {
        Ok(app) => (StatusCode::OK, Json(serde_json::json!(app))).into_response(),
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            not_found(format!("app '{name}' not found")).into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

async fn delete_app(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> impl IntoResponse {
    match queries::delete_app(&state.pool, &name).await {
        Ok(()) => {
            if let Err(e) =
                crate::proxy::remove_app_config(&state.config.angie_conf_dir, &name)
            {
                tracing::warn!("failed to remove angie config for {name}: {e}");
            }
            if let Err(e) = crate::proxy::reload().await {
                tracing::warn!("angie reload failed after app delete: {e}");
            }
            state.events.emit(
                None,
                "app.deleted",
                Some(serde_json::json!({ "name": name })),
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            not_found(format!("app '{name}' not found")).into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

// ── Events ────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct EventQuery {
    app: Option<String>,
    since: Option<DateTime<Utc>>,
}

async fn list_events(
    State(state): State<SharedState>,
    Query(params): Query<EventQuery>,
) -> impl IntoResponse {
    match queries::list_events(&state.pool, params.app.as_deref(), params.since).await {
        Ok(events) => (StatusCode::OK, Json(serde_json::json!(events))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

async fn stream_events(
    State(state): State<SharedState>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let rx = state.events.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(event) => serde_json::to_string(&event)
            .ok()
            .map(|data| Ok(SseEvent::default().data(data))),
        Err(_) => None,
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ── Domains ───────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AddDomainBody {
    domain: String,
}

async fn list_domains(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> impl IntoResponse {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(a) => a,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };
    match queries::list_domains(&state.pool, &app.id).await {
        Ok(domains) => (StatusCode::OK, Json(serde_json::json!(domains))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

async fn add_domain(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(body): Json<AddDomainBody>,
) -> impl IntoResponse {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(a) => a,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };

    match queries::add_domain(&state.pool, &app.id, &body.domain).await {
        Ok(domain) => {
            rewrite_and_reload(&state, &app.id, &name).await;
            state.events.emit(
                Some(app.id.clone()),
                "domain.added",
                Some(serde_json::json!({ "domain": body.domain })),
            );
            (StatusCode::CREATED, Json(serde_json::json!(domain))).into_response()
        }
        Err(e) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn remove_domain(
    State(state): State<SharedState>,
    axum::extract::Path((name, domain)): axum::extract::Path<(String, String)>,
) -> impl IntoResponse {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(a) => a,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };

    match queries::remove_domain(&state.pool, &app.id, &domain).await {
        Ok(()) => {
            rewrite_and_reload(&state, &app.id, &name).await;
            state.events.emit(
                Some(app.id.clone()),
                "domain.removed",
                Some(serde_json::json!({ "domain": domain })),
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => not_found(e).into_response(),
    }
}

// ── Ports ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AddPortBody {
    host_port: i64,
    container_port: i64,
    #[serde(default = "default_protocol")]
    protocol: String,
}

fn default_protocol() -> String {
    "tcp".to_string()
}

async fn list_ports(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> impl IntoResponse {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(a) => a,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };
    match queries::list_port_mappings(&state.pool, &app.id).await {
        Ok(ports) => (StatusCode::OK, Json(serde_json::json!(ports))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

async fn add_port(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(body): Json<AddPortBody>,
) -> impl IntoResponse {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(a) => a,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };

    match queries::add_port_mapping(
        &state.pool,
        &app.id,
        body.host_port,
        body.container_port,
        &body.protocol,
    )
    .await
    {
        Ok(port) => {
            state.events.emit(
                Some(app.id.clone()),
                "port.added",
                Some(serde_json::json!({ "host_port": body.host_port, "container_port": body.container_port })),
            );
            (StatusCode::CREATED, Json(serde_json::json!(port))).into_response()
        }
        Err(e) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn remove_port(
    State(state): State<SharedState>,
    axum::extract::Path((name, id)): axum::extract::Path<(String, String)>,
) -> impl IntoResponse {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(a) => a,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };

    match queries::remove_port_mapping(&state.pool, &app.id, &id).await {
        Ok(()) => {
            state.events.emit(
                Some(app.id),
                "port.removed",
                Some(serde_json::json!({ "id": id })),
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => not_found(e).into_response(),
    }
}

// ── Routing table ─────────────────────────────────────────────────────────────

async fn list_routing(State(state): State<SharedState>) -> impl IntoResponse {
    match build_routing_table(&state).await {
        Ok(table) => (StatusCode::OK, Json(serde_json::json!(table))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct UpdateRoutingBody {
    upstreams: Vec<Upstream>,
}

/// Update the upstream(s) for an app and rewrite the Angie config.
async fn update_routing(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(body): Json<UpdateRoutingBody>,
) -> impl IntoResponse {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(a) => a,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };

    let domains = match queries::list_domain_names(&state.pool, &app.id).await {
        Ok(d) => d,
        Err(e) => return internal_error(e).into_response(),
    };

    let result = crate::proxy::write_app_config(
        &state.config.angie_conf_dir,
        &name,
        &domains,
        &body.upstreams,
        false, // TLS managed separately
    );

    if let Err(e) = result {
        return internal_error(e).into_response();
    }

    if let Err(e) = crate::proxy::reload().await {
        tracing::warn!("angie reload failed: {e}");
    }

    state.events.emit(
        Some(app.id),
        "routing.updated",
        Some(serde_json::json!({ "upstreams": body.upstreams })),
    );

    StatusCode::NO_CONTENT.into_response()
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Rewrite the Angie config for an app after domain/port changes.
/// Errors are logged but not propagated — config writes should not fail user-facing ops.
async fn rewrite_and_reload(state: &AppState, app_id: &str, app_name: &str) {
    let domains = match queries::list_domain_names(&state.pool, app_id).await {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("failed to fetch domains for angie rewrite: {e}");
            return;
        }
    };

    let ports = match queries::list_port_mappings(&state.pool, app_id).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("failed to fetch ports for angie rewrite: {e}");
            return;
        }
    };

    if domains.is_empty() || ports.is_empty() {
        return;
    }

    let upstreams: Vec<Upstream> = ports
        .iter()
        .map(|p| Upstream {
            host: "127.0.0.1".to_string(),
            port: p.host_port as u16,
        })
        .collect();

    if let Err(e) =
        crate::proxy::write_app_config(&state.config.angie_conf_dir, app_name, &domains, &upstreams, false)
    {
        tracing::error!("failed to write angie config: {e}");
        return;
    }

    if let Err(e) = crate::proxy::reload().await {
        tracing::warn!("angie reload failed: {e}");
    }
}

// ── Deployments ───────────────────────────────────────────────────────────────

async fn list_deployments(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> impl IntoResponse {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(a) => a,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };
    match queries::list_deployments(&state.pool, &app.id).await {
        Ok(deps) => (StatusCode::OK, Json(serde_json::json!(deps))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct DeployBody {
    /// "source" | "image" | "archive"
    source: String,
    /// Image reference (when source = "image")
    image: Option<String>,
    /// Archive path or URL (when source = "archive")
    archive: Option<String>,
    /// Force a specific builder
    builder: Option<String>,
}

async fn trigger_deploy(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(body): Json<DeployBody>,
) -> impl IntoResponse {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(a) => a,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };

    if app.locked {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "app is locked" })),
        )
            .into_response();
    }

    let deploy_source = match body.source.as_str() {
        "image" => {
            let image = match body.image {
                Some(i) => i,
                None => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({ "error": "image is required for source=image" })),
                    )
                        .into_response();
                }
            };
            DeploySource::Image { reference: image }
        }
        "archive" => {
            let archive = match body.archive {
                Some(a) => a,
                None => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({ "error": "archive is required for source=archive" })),
                    )
                        .into_response();
                }
            };
            DeploySource::Archive { path: archive }
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "source must be 'image' or 'archive'" })),
            )
                .into_response();
        }
    };

    let req = DeployRequest {
        app_id: app.id.clone(),
        app_name: name.clone(),
        source: deploy_source,
        force_builder: body.builder,
    };

    let pool = state.pool.clone();
    let docker = state.docker.clone();
    let events = state.events.clone();
    let cfg = state.config.clone();
    let app_id = app.id.clone();

    // Run deploy in background so the HTTP response returns immediately
    tokio::spawn(async move {
        if let Err(e) = crate::deploy::run_deploy(&pool, &docker, &events, &cfg, req).await {
            tracing::error!(app = %app_id, "deploy failed: {e}");
        }
    });

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "message": "deploy started", "app": name })),
    )
        .into_response()
}

#[derive(Debug, Deserialize)]
struct RollbackBody {
    deployment_id: Option<String>,
}

async fn trigger_rollback(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(body): Json<RollbackBody>,
) -> impl IntoResponse {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(a) => a,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };

    let pool = state.pool.clone();
    let docker = state.docker.clone();
    let events = state.events.clone();
    let cfg = state.config.clone();
    let app_id = app.id.clone();
    let to_id = body.deployment_id.clone();

    tokio::spawn(async move {
        if let Err(e) = crate::deploy::rollback(
            &pool,
            &docker,
            &events,
            &cfg,
            &app_id,
            &name,
            to_id.as_deref(),
        )
        .await
        {
            tracing::error!(app = %app_id, "rollback failed: {e}");
        }
    });

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "message": "rollback started" })),
    )
        .into_response()
}

// ── Logs ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct LogsQuery {
    #[serde(default = "default_log_lines")]
    n: usize,
}

fn default_log_lines() -> usize {
    100
}

async fn get_logs(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Query(params): Query<LogsQuery>,
) -> impl IntoResponse {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(a) => a,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };

    let containers = match queries::list_containers_for_app(&state.pool, &app.id).await {
        Ok(c) => c,
        Err(e) => return internal_error(e).into_response(),
    };

    let mut all_logs = Vec::new();
    for c in containers {
        match crate::container::get_container_logs(&state.docker, &c.id, params.n).await {
            Ok(lines) => all_logs.extend(lines),
            Err(e) => tracing::warn!("failed to get logs for container {}: {e}", c.id),
        }
    }

    (StatusCode::OK, Json(serde_json::json!({ "logs": all_logs }))).into_response()
}

// ── Config vars ───────────────────────────────────────────────────────────────

async fn list_config(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> impl IntoResponse {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(a) => a,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };
    match queries::get_config_vars(&state.pool, &app.id).await {
        Ok(vars) => (StatusCode::OK, Json(serde_json::json!(vars))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct SetConfigBody {
    key: String,
    value: String,
    #[serde(default)]
    is_global: bool,
}

async fn set_config(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(body): Json<SetConfigBody>,
) -> impl IntoResponse {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(a) => a,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };
    match queries::set_config_var(&state.pool, &app.id, &body.key, &body.value, body.is_global)
        .await
    {
        Ok(()) => {
            state.events.emit(
                Some(app.id),
                "config.set",
                Some(serde_json::json!({ "key": body.key })),
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

async fn unset_config(
    State(state): State<SharedState>,
    axum::extract::Path((name, key)): axum::extract::Path<(String, String)>,
) -> impl IntoResponse {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(a) => a,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };
    match queries::unset_config_var(&state.pool, &app.id, &key).await {
        Ok(()) => {
            state.events.emit(
                Some(app.id),
                "config.unset",
                Some(serde_json::json!({ "key": key })),
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

// ── Process scale ─────────────────────────────────────────────────────────────

async fn get_scale(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> impl IntoResponse {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(a) => a,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };
    match queries::get_process_scales(&state.pool, &app.id).await {
        Ok(scales) => (StatusCode::OK, Json(serde_json::json!(scales))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct SetScaleBody {
    /// Map of process_type → count
    scales: std::collections::HashMap<String, i64>,
}

async fn set_scale(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(body): Json<SetScaleBody>,
) -> impl IntoResponse {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(a) => a,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };
    for (proc_type, count) in &body.scales {
        if let Err(e) =
            queries::set_process_scale(&state.pool, &app.id, proc_type, *count).await
        {
            return internal_error(e).into_response();
        }
    }
    state.events.emit(
        Some(app.id),
        "scale.updated",
        Some(serde_json::json!({ "scales": body.scales })),
    );
    StatusCode::NO_CONTENT.into_response()
}

// ── Internal helpers ──────────────────────────────────────────────────────────

async fn build_routing_table(
    state: &AppState,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let apps = queries::list_apps(&state.pool).await?;
    let mut table = Vec::new();

    for app in apps {
        let domains = queries::list_domain_names(&state.pool, &app.id).await?;
        let ports = queries::list_port_mappings(&state.pool, &app.id).await?;
        table.push(serde_json::json!({
            "app": app.name,
            "domains": domains,
            "upstreams": ports.iter().map(|p| serde_json::json!({
                "host": "127.0.0.1",
                "port": p.host_port,
            })).collect::<Vec<_>>(),
        }));
    }

    Ok(table)
}
