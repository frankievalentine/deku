use std::convert::Infallible;
use std::future::IntoFuture;
use std::sync::Arc;

use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    middleware,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use futures::Stream;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::net::{TcpListener, UnixListener};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::config::DekuConfig;
use crate::container::DockerClient;
use crate::db::queries;
use crate::deploy::{DeployRequest, DeploySource};
use crate::events::EventSender;
use deku_core::types::{NewApp, ObjectStoreConfig, Upstream};

pub mod auth;
mod services;

#[derive(Clone)]
pub struct AppState {
    pub config: DekuConfig,
    pub pool: SqlitePool,
    pub events: EventSender,
    pub docker: DockerClient,
    pub plugins: std::sync::Arc<crate::plugins::PluginRegistry>,
}

impl AppState {
    pub fn new(
        config: DekuConfig,
        pool: SqlitePool,
        events: EventSender,
        docker: DockerClient,
        plugins: std::sync::Arc<crate::plugins::PluginRegistry>,
    ) -> Arc<Self> {
        Arc::new(Self {
            config,
            pool,
            events,
            docker,
            plugins,
        })
    }
}

pub type SharedState = Arc<AppState>;

fn build_router(state: SharedState) -> Router {
    let dashboard_dir = state.config.dashboard_dir.clone();
    Router::new()
        // Health
        .route("/healthz", get(health_check))
        // Apps
        .route("/api/apps", get(list_apps).post(create_app))
        .route("/api/apps/{name}", get(get_app).delete(delete_app))
        // Events
        .route("/api/events", get(list_events))
        .route("/api/events/stream", get(stream_events))
        .route("/api/apps/{name}/events/stream", get(stream_app_events))
        // Domains
        .route(
            "/api/apps/{name}/domains",
            get(list_domains).post(add_domain),
        )
        .route("/api/apps/{name}/domains/{domain}", delete(remove_domain))
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
        .route("/api/apps/{name}/config", get(list_config).post(set_config))
        .route("/api/apps/{name}/config/{key}", delete(unset_config))
        // Process scale
        .route("/api/apps/{name}/ps", get(list_processes))
        .route("/api/apps/{name}/scale", get(get_scale).post(set_scale))
        // Routing table
        .route("/api/routing", get(list_routing))
        .route("/api/routing/status", get(get_routing_status))
        .route(
            "/api/routing/status/{name}",
            get(get_routing_status_for_app),
        )
        .route("/api/routing/{name}", post(update_routing))
        // SSH keys
        .route("/api/ssh-keys", get(list_ssh_keys).post(add_ssh_key))
        .route("/api/ssh-keys/{name}", delete(remove_ssh_key))
        // Plugins
        .route("/api/plugins", get(list_plugins).post(install_plugin))
        .route("/api/plugins/{name}", delete(uninstall_plugin))
        // Object store
        .route(
            "/api/objectstore",
            get(get_object_store_config)
                .post(set_object_store_config)
                .delete(unset_object_store_config),
        )
        .route("/api/objectstore/test", post(test_object_store_config))
        // Archive deploy
        .route("/api/apps/{name}/deploy/archive", post(deploy_archive))
        // Postgres
        .route(
            "/api/postgres/services",
            get(services::pg_list).post(services::pg_create),
        )
        .route(
            "/api/postgres/services/{name}",
            get(services::pg_info).delete(services::pg_destroy),
        )
        .route(
            "/api/postgres/services/{name}/backups",
            get(services::pg_backups).post(services::pg_backup),
        )
        .route(
            "/api/postgres/services/{name}/restore/{backup_id}",
            post(services::pg_restore),
        )
        .route(
            "/api/postgres/services/{name}/link/{app}",
            post(services::pg_link).delete(services::pg_unlink),
        )
        .route("/api/postgres/services/{name}/logs", get(services::pg_logs))
        // Redis
        .route(
            "/api/redis/services",
            get(services::rd_list).post(services::rd_create),
        )
        .route(
            "/api/redis/services/{name}",
            get(services::rd_info).delete(services::rd_destroy),
        )
        .route(
            "/api/redis/services/{name}/link/{app}",
            post(services::rd_link).delete(services::rd_unlink),
        )
        .route("/api/redis/services/{name}/logs", get(services::rd_logs))
        // MySQL
        .route(
            "/api/mysql/services",
            get(services::my_list).post(services::my_create),
        )
        .route(
            "/api/mysql/services/{name}",
            get(services::my_info).delete(services::my_destroy),
        )
        .route(
            "/api/mysql/services/{name}/link/{app}",
            post(services::my_link).delete(services::my_unlink),
        )
        .route("/api/mysql/services/{name}/logs", get(services::my_logs))
        // Letsencrypt
        .route("/api/letsencrypt/enable/{app}", post(services::le_enable))
        .route("/api/letsencrypt/disable/{app}", post(services::le_disable))
        .route("/api/letsencrypt/status/{app}", get(services::le_status))
        .route(
            "/api/letsencrypt/config",
            get(services::le_get_config).post(services::le_config),
        )
        // Networks
        .route(
            "/api/networks",
            get(services::net_list).post(services::net_create),
        )
        .route("/api/networks/{name}", delete(services::net_destroy))
        .route("/api/apps/{app}/networks", get(services::net_list_for_app))
        .route(
            "/api/apps/{app}/networks/{network}",
            post(services::net_attach).delete(services::net_detach),
        )
        // Storage
        .route(
            "/api/apps/{app}/storage",
            get(services::storage_list).post(services::storage_add),
        )
        .route(
            "/api/apps/{app}/storage/ensure",
            post(services::storage_ensure),
        )
        .route(
            "/api/apps/{app}/storage/{id}",
            delete(services::storage_remove),
        )
        // Cron
        .route(
            "/api/apps/{app}/cron",
            get(services::cron_list).post(services::cron_add),
        )
        .route("/api/apps/{app}/cron/{id}", delete(services::cron_remove))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        // Dashboard static files — fallback so API routes take precedence
        .fallback_service(ServeDir::new(dashboard_dir).append_index_html_on_directories(true))
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

// ── Object Store ──────────────────────────────────────────────────────────────

async fn get_object_store_config() -> impl IntoResponse {
    match crate::config::load() {
        Ok(cfg) => {
            let payload = cfg.object_store.map(|object_store| object_store.redacted());
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "configured": payload.is_some(),
                    "object_store": payload,
                })),
            )
                .into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

async fn set_object_store_config(Json(body): Json<ObjectStoreConfig>) -> impl IntoResponse {
    let response_body = body.redacted();
    match crate::config::load().and_then(|mut cfg| {
        cfg.object_store = Some(body);
        crate::config::save(&cfg)?;
        Ok(())
    }) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "configured": true,
                "object_store": response_body,
            })),
        )
            .into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

async fn unset_object_store_config() -> impl IntoResponse {
    match crate::config::load().and_then(|mut cfg| {
        cfg.object_store = None;
        crate::config::save(&cfg)?;
        Ok(())
    }) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

async fn test_object_store_config() -> impl IntoResponse {
    match crate::config::load() {
        Ok(cfg) => match cfg.object_store {
            Some(object_store) => match crate::objectstore::test_config(&object_store).await {
                Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response(),
                Err(e) => (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
                    .into_response(),
            },
            None => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "object store is not configured" })),
            )
                .into_response(),
        },
        Err(e) => internal_error(e).into_response(),
    }
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
            if let Err(e) = crate::proxy::remove_app_config(&state.config.angie_conf_dir, &name) {
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

#[derive(Debug, Deserialize)]
struct StreamQuery {
    since: Option<String>,
}

async fn stream_app_events(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Query(params): Query<StreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<SseEvent, Infallible>>>, Response> {
    let _ = params.since;
    let app = queries::get_app(&state.pool, &name)
        .await
        .map_err(|err| match err {
            deku_core::error::DekuError::AppNotFound(_) => {
                not_found(format!("app '{name}' not found")).into_response()
            }
            other => internal_error(other).into_response(),
        })?;

    let app_id = app.id;
    let rx = state.events.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(move |result| {
        let app_id = app_id.clone();
        match result {
            Ok(event) if event.app_id.as_ref() == Some(&app_id) => serde_json::to_string(&event)
                .ok()
                .map(|data| Ok(SseEvent::default().data(data))),
            _ => None,
        }
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
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
            if let Err(e) = reconcile_proxy_for_app(&state, &app.id, &name).await {
                if let Err(rollback_error) =
                    queries::remove_domain(&state.pool, &app.id, &body.domain).await
                {
                    tracing::error!(
                        app = %app.id,
                        domain = %body.domain,
                        "failed to roll back domain add after proxy reconciliation error: {rollback_error}"
                    );
                }
                return internal_error(e).into_response();
            }
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

    let domain_record = match queries::list_domains(&state.pool, &app.id).await {
        Ok(domains) => domains.into_iter().find(|item| item.domain == domain),
        Err(e) => return internal_error(e).into_response(),
    };

    match queries::remove_domain(&state.pool, &app.id, &domain).await {
        Ok(()) => {
            if let Err(e) = reconcile_proxy_for_app(&state, &app.id, &name).await {
                if let Some(previous) = domain_record {
                    if let Err(rollback_error) =
                        queries::add_domain(&state.pool, &app.id, &previous.domain).await
                    {
                        tracing::error!(
                            app = %app.id,
                            domain = %previous.domain,
                            "failed to roll back domain removal after proxy reconciliation error: {rollback_error}"
                        );
                    }
                }
                return internal_error(e).into_response();
            }
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
            if let Err(e) = reconcile_proxy_for_app(&state, &app.id, &name).await {
                if let Err(rollback_error) =
                    queries::remove_port_mapping(&state.pool, &app.id, &port.id).await
                {
                    tracing::error!(
                        app = %app.id,
                        port_id = %port.id,
                        "failed to roll back port add after proxy reconciliation error: {rollback_error}"
                    );
                }
                return internal_error(e).into_response();
            }
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

    let existing_port = match queries::list_port_mappings(&state.pool, &app.id).await {
        Ok(ports) => ports.into_iter().find(|port| port.id == id),
        Err(e) => return internal_error(e).into_response(),
    };

    match queries::remove_port_mapping(&state.pool, &app.id, &id).await {
        Ok(()) => {
            if let Err(e) = reconcile_proxy_for_app(&state, &app.id, &name).await {
                if let Some(previous) = existing_port {
                    if let Err(rollback_error) = queries::add_port_mapping(
                        &state.pool,
                        &app.id,
                        previous.host_port,
                        previous.container_port,
                        &previous.protocol,
                    )
                    .await
                    {
                        tracing::error!(
                            app = %app.id,
                            host_port = previous.host_port,
                            container_port = previous.container_port,
                            "failed to roll back port removal after proxy reconciliation error: {rollback_error}"
                        );
                    }
                }
                return internal_error(e).into_response();
            }
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

#[derive(Debug, Serialize)]
struct AngieConfigStatus {
    config_valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    validation_error: Option<String>,
}

#[derive(Debug, Serialize)]
struct RoutingAppStatus {
    app: String,
    status: String,
    domains: Vec<String>,
    upstreams: Vec<Upstream>,
    tls_enabled: bool,
    proxy_config_path: String,
    proxy_config_present: bool,
    certificate: crate::services::letsencrypt::FileStatus,
    private_key: crate::services::letsencrypt::FileStatus,
    tls_ready: bool,
    issues: Vec<String>,
}

#[derive(Debug, Serialize)]
struct RoutingStatusResponse {
    angie: AngieConfigStatus,
    apps: Vec<RoutingAppStatus>,
}

#[derive(Debug, Serialize)]
struct SingleRoutingStatusResponse {
    angie: AngieConfigStatus,
    app: RoutingAppStatus,
}

async fn get_routing_status(State(state): State<SharedState>) -> impl IntoResponse {
    match build_routing_status_response(&state).await {
        Ok(status) => (StatusCode::OK, Json(serde_json::json!(status))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

async fn get_routing_status_for_app(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> impl IntoResponse {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(app) => app,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };

    let angie = angie_config_status().await;
    match build_routing_status_for_app(&state, &app).await {
        Ok(app_status) => (
            StatusCode::OK,
            Json(serde_json::json!(SingleRoutingStatusResponse {
                angie,
                app: app_status,
            })),
        )
            .into_response(),
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

    let desired = if domains.is_empty() || body.upstreams.is_empty() {
        None
    } else {
        Some(crate::proxy::DesiredAppConfig {
            domains: &domains,
            upstreams: &body.upstreams,
            tls: app.tls_enabled,
        })
    };

    if let Err(e) =
        crate::proxy::apply_app_config(&state.config.angie_conf_dir, &name, desired).await
    {
        return internal_error(e).into_response();
    }

    state.events.emit(
        Some(app.id),
        "routing.updated",
        Some(serde_json::json!({ "upstreams": body.upstreams })),
    );

    StatusCode::NO_CONTENT.into_response()
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Reconcile the Angie config for an app after domain/port changes.
async fn reconcile_proxy_for_app(
    state: &AppState,
    app_id: &str,
    app_name: &str,
) -> anyhow::Result<()> {
    let tls = match queries::get_app_by_id(&state.pool, app_id).await {
        Ok(app) => app.tls_enabled,
        Err(e) => return Err(e.into()),
    };

    let domains = queries::list_domain_names(&state.pool, app_id).await?;
    let ports = queries::list_port_mappings(&state.pool, app_id).await?;

    if domains.is_empty() || ports.is_empty() {
        crate::proxy::apply_app_config(&state.config.angie_conf_dir, app_name, None).await?;
        return Ok(());
    }

    let upstreams = upstreams_from_ports(&ports);
    crate::proxy::apply_app_config(
        &state.config.angie_conf_dir,
        app_name,
        Some(crate::proxy::DesiredAppConfig {
            domains: &domains,
            upstreams: &upstreams,
            tls,
        }),
    )
    .await?;
    Ok(())
}

fn upstreams_from_ports(ports: &[deku_core::types::PortMapping]) -> Vec<Upstream> {
    ports
        .iter()
        .map(|p| Upstream {
            host: "127.0.0.1".to_string(),
            port: p.host_port as u16,
        })
        .collect()
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

    (
        StatusCode::OK,
        Json(serde_json::json!({ "logs": all_logs })),
    )
        .into_response()
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

// ── Process inspection / scale ────────────────────────────────────────────────

async fn list_processes(
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

    let scales = match queries::get_process_scales(&state.pool, &app.id).await {
        Ok(scales) => scales,
        Err(e) => return internal_error(e).into_response(),
    };

    match queries::list_containers_for_app(&state.pool, &app.id).await {
        Ok(containers) => {
            let processes: Vec<_> = containers
                .into_iter()
                .map(|container| {
                    let scale = scales.get(&container.process_type).copied().unwrap_or(1);
                    serde_json::json!({
                        "process_type": container.process_type,
                        "scale": scale,
                        "status": container.status,
                        "container_id": container.id,
                        "deployment_id": container.deployment_id,
                        "host_port": container.host_port,
                        "created_at": container.created_at,
                    })
                })
                .collect();

            (StatusCode::OK, Json(serde_json::json!(processes))).into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

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
        if let Err(e) = queries::set_process_scale(&state.pool, &app.id, proc_type, *count).await {
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

async fn build_routing_table(state: &AppState) -> anyhow::Result<Vec<serde_json::Value>> {
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

async fn build_routing_status_response(state: &AppState) -> anyhow::Result<RoutingStatusResponse> {
    let apps = queries::list_apps(&state.pool).await?;
    let angie = angie_config_status().await;
    let mut statuses = Vec::with_capacity(apps.len());

    for app in apps {
        statuses.push(build_routing_status_for_app(state, &app).await?);
    }

    Ok(RoutingStatusResponse {
        angie,
        apps: statuses,
    })
}

async fn build_routing_status_for_app(
    state: &AppState,
    app: &deku_core::types::App,
) -> anyhow::Result<RoutingAppStatus> {
    let domains = queries::list_domain_names(&state.pool, &app.id).await?;
    let ports = queries::list_port_mappings(&state.pool, &app.id).await?;
    let upstreams = upstreams_from_ports(&ports);
    let proxy_config_path = crate::proxy::app_config_path(&state.config.angie_conf_dir, &app.name);
    let proxy_config_present = proxy_config_path.exists();
    let tls_status = crate::services::letsencrypt::status(&state.pool, &app.name).await?;
    let mut issues = Vec::new();
    let inputs_complete = !domains.is_empty() && !upstreams.is_empty();

    if domains.is_empty() {
        issues.push("no domains configured".to_string());
    }
    if upstreams.is_empty() {
        issues.push("no upstreams configured".to_string());
    }
    if inputs_complete && !proxy_config_present {
        issues.push("angie config fragment is missing".to_string());
    }
    if !inputs_complete && proxy_config_present {
        issues.push("angie config fragment exists without complete routing inputs".to_string());
    }
    if app.tls_enabled && !tls_status.certificate.exists {
        issues.push(format!(
            "certificate file missing: {}",
            tls_status.certificate.path
        ));
    }
    if app.tls_enabled && !tls_status.private_key.exists {
        issues.push(format!(
            "private key file missing: {}",
            tls_status.private_key.path
        ));
    }
    if let Some(error) = &tls_status.inspection_error {
        issues.push(format!("certificate inspection failed: {error}"));
    }

    let status = if issues.is_empty() {
        "ready"
    } else if !inputs_complete {
        "pending"
    } else {
        "degraded"
    };

    Ok(RoutingAppStatus {
        app: app.name.clone(),
        status: status.to_string(),
        domains,
        upstreams,
        tls_enabled: app.tls_enabled,
        proxy_config_path: proxy_config_path.display().to_string(),
        proxy_config_present,
        certificate: tls_status.certificate,
        private_key: tls_status.private_key,
        tls_ready: tls_status.ready,
        issues,
    })
}

async fn angie_config_status() -> AngieConfigStatus {
    match crate::proxy::reloader::validate().await {
        Ok(()) => AngieConfigStatus {
            config_valid: true,
            validation_error: None,
        },
        Err(error) => AngieConfigStatus {
            config_valid: false,
            validation_error: Some(error.to_string()),
        },
    }
}

// ── SSH Keys ──────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AddSshKeyBody {
    name: String,
    public_key: String,
}

async fn list_ssh_keys(State(state): State<SharedState>) -> impl IntoResponse {
    match queries::list_ssh_keys(&state.pool).await {
        Ok(keys) => {
            let json: Vec<_> = keys
                .iter()
                .map(|k| {
                    serde_json::json!({
                        "id": k.id,
                        "name": k.name,
                        "fingerprint": k.fingerprint,
                    })
                })
                .collect();
            (StatusCode::OK, Json(serde_json::json!(json))).into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

async fn add_ssh_key(
    State(state): State<SharedState>,
    Json(body): Json<AddSshKeyBody>,
) -> impl IntoResponse {
    use russh::keys::ssh_key::{HashAlg, PublicKey};

    let parsed = match PublicKey::from_openssh(&body.public_key) {
        Ok(k) => k,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("invalid public key: {e}") })),
            )
                .into_response();
        }
    };
    let fingerprint = parsed.fingerprint(HashAlg::Sha256).to_string();

    match queries::add_ssh_key(&state.pool, &body.name, &body.public_key, &fingerprint).await {
        Ok(key) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "id": key.id,
                "name": key.name,
                "fingerprint": key.fingerprint,
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn remove_ssh_key(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> impl IntoResponse {
    match queries::remove_ssh_key(&state.pool, &name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => not_found(e).into_response(),
    }
}

// ── Plugins ───────────────────────────────────────────────────────────────────

async fn list_plugins(State(state): State<SharedState>) -> impl IntoResponse {
    let plugins = state.plugins.list_plugins().await;
    let json: Vec<_> = plugins
        .iter()
        .map(|(name, version, path)| {
            serde_json::json!({
                "name": name,
                "version": version,
                "path": path.display().to_string(),
            })
        })
        .collect();
    (StatusCode::OK, Json(serde_json::json!(json))).into_response()
}

#[derive(Debug, Deserialize)]
struct InstallPluginBody {
    path: String,
}

async fn install_plugin(
    State(state): State<SharedState>,
    Json(body): Json<InstallPluginBody>,
) -> impl IntoResponse {
    let path = std::path::Path::new(&body.path);
    match state.plugins.load_plugin(path).await {
        Ok(name) => (
            StatusCode::CREATED,
            Json(serde_json::json!({ "name": name })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn uninstall_plugin(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> impl IntoResponse {
    match state.plugins.unload_plugin(&name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => not_found(e).into_response(),
    }
}

// ── Archive deploy ────────────────────────────────────────────────────────────

fn write_temp_archive(bytes: &[u8]) -> std::io::Result<std::path::PathBuf> {
    use std::io::Write;
    let mut tmp = tempfile::NamedTempFile::new()?;
    tmp.write_all(bytes)?;
    let (_, path) = tmp.keep().map_err(|e| e.error)?;
    Ok(path)
}

#[derive(Debug, Deserialize)]
struct ArchiveDeployQuery {
    builder: Option<String>,
}

async fn deploy_archive(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Query(params): Query<ArchiveDeployQuery>,
    mut multipart: axum::extract::Multipart,
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

    // Read archive bytes from multipart field named "archive"
    let mut archive_bytes: Option<Vec<u8>> = None;
    loop {
        match multipart.next_field().await {
            Ok(Some(field)) if field.name() == Some("archive") => match field.bytes().await {
                Ok(b) => {
                    archive_bytes = Some(b.to_vec());
                    break;
                }
                Err(e) => return internal_error(e).into_response(),
            },
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(e) => return internal_error(e).into_response(),
        }
    }

    let bytes = match archive_bytes {
        Some(b) => b,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "missing 'archive' field in multipart body" })),
            )
                .into_response();
        }
    };

    let tmp_path = match write_temp_archive(&bytes) {
        Ok(p) => p,
        Err(e) => return internal_error(e).into_response(),
    };
    let path_str = tmp_path.to_string_lossy().to_string();

    let req = DeployRequest {
        app_id: app.id.clone(),
        app_name: name.clone(),
        source: DeploySource::Archive {
            path: path_str.clone(),
        },
        force_builder: params.builder,
    };

    let pool = state.pool.clone();
    let docker = state.docker.clone();
    let events = state.events.clone();
    let cfg = state.config.clone();
    let app_id = app.id.clone();

    tokio::spawn(async move {
        if let Err(e) = crate::deploy::run_deploy(&pool, &docker, &events, &cfg, req).await {
            tracing::error!(app = %app_id, "archive deploy failed: {e}");
        }
        let _ = std::fs::remove_file(&path_str);
    });

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "message": "deploy started", "app": name })),
    )
        .into_response()
}
