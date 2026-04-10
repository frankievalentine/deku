use std::convert::Infallible;
use std::future::IntoFuture;
use std::sync::Arc;

use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    middleware,
    response::{Html, IntoResponse, Response},
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
use crate::version::{self, CachedVersionStatus};
use deku_core::{
    auth::{issue_dashboard_token, DashboardTokenState},
    types::{NewApp, ObjectStoreConfig, Upstream},
};
use deku_plugin_sdk::context::AppContext;
use tokio::sync::RwLock;

pub mod auth;
mod services;

pub struct AppState {
    pub config: DekuConfig,
    pub dashboard_auth: RwLock<Option<DashboardTokenState>>,
    pub version_status: RwLock<Option<CachedVersionStatus>>,
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
            dashboard_auth: RwLock::new(config.dashboard_auth.clone()),
            version_status: RwLock::new(None),
            config,
            pool,
            events,
            docker,
            plugins,
        })
    }
}

pub type SharedState = Arc<AppState>;

fn build_api_router(state: SharedState) -> Router {
    Router::new()
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
        .route("/api/apps/{name}/checks", get(get_app_checks))
        // Config vars
        .route("/api/apps/{name}/config", get(list_config).post(set_config))
        .route("/api/apps/{name}/config/{key}", delete(unset_config))
        .route(
            "/api/apps/{name}/objectstore",
            get(get_app_object_store_link)
                .post(link_app_object_store)
                .delete(unlink_app_object_store),
        )
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
        // Dashboard auth
        .route("/api/dashboard/token", post(rotate_dashboard_token))
        .route("/api/version", get(get_version_status))
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
            "/api/redis/services/{name}/backups",
            get(services::rd_backups).post(services::rd_backup),
        )
        .route(
            "/api/redis/services/{name}/restore/{backup_id}",
            post(services::rd_restore),
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
}

fn build_public_router(state: SharedState) -> Router {
    let dashboard_dir = state.config.dashboard_dir.clone();
    let router = Router::new().route("/healthz", get(health_check));

    if crate::config::dashboard_assets_available(&state.config) {
        router
            .fallback_service(ServeDir::new(dashboard_dir).append_index_html_on_directories(true))
            .with_state(state)
    } else {
        router.fallback(missing_dashboard_page).with_state(state)
    }
}

async fn missing_dashboard_page(State(state): State<SharedState>) -> impl IntoResponse {
    let dashboard_dir = escape_html(&state.config.dashboard_dir.display().to_string());
    let body = format!(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Deku Dashboard Assets Missing</title>
    <style>
      :root {{
        color-scheme: light dark;
        font-family:
          ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      }}
      body {{
        margin: 0;
        min-height: 100vh;
        display: grid;
        place-items: center;
        background: #0f172a;
        color: #e2e8f0;
      }}
      main {{
        width: min(42rem, calc(100vw - 2rem));
        padding: 2rem;
        border: 1px solid rgba(148, 163, 184, 0.25);
        border-radius: 1rem;
        background: rgba(15, 23, 42, 0.88);
        box-shadow: 0 24px 80px rgba(15, 23, 42, 0.45);
      }}
      h1 {{
        margin: 0 0 0.75rem;
        font-size: clamp(1.8rem, 5vw, 2.4rem);
      }}
      p, li {{
        line-height: 1.6;
        color: #cbd5e1;
      }}
      code, pre {{
        font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
      }}
      pre {{
        overflow-x: auto;
        padding: 1rem;
        border-radius: 0.75rem;
        background: rgba(15, 23, 42, 0.92);
        border: 1px solid rgba(148, 163, 184, 0.2);
      }}
      .eyebrow {{
        margin: 0 0 0.6rem;
        text-transform: uppercase;
        letter-spacing: 0.08em;
        font-size: 0.8rem;
        color: #94a3b8;
      }}
    </style>
  </head>
  <body>
    <main>
      <p class="eyebrow">Deku</p>
      <h1>Dashboard assets are not staged</h1>
      <p>
        <code>dekud</code> is running, but it did not find a built dashboard bundle at
        <code>{dashboard_dir}</code>.
      </p>
      <p>Restore the dashboard bundle in one of these ways:</p>
      <ul>
        <li>Packaged install: rerun the installer or restage the extracted <code>deku-dashboard.tar.gz</code> contents into the configured dashboard directory.</li>
        <li>Source checkout: run <code>cd dashboard &amp;&amp; bun run build</code>, then copy <code>dashboard/dist</code> into the configured dashboard directory or point <code>dashboard_dir</code> at that build output in <code>~/.deku/config.toml</code>.</li>
      </ul>
      <pre>dashboard_dir = "{dashboard_dir}"</pre>
    </main>
  </body>
</html>"#
    );

    (StatusCode::SERVICE_UNAVAILABLE, Html(body))
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub async fn serve(state: SharedState) -> anyhow::Result<()> {
    // Unix socket router — trusted local access, no auth required
    let unix_app = build_api_router(state.clone())
        .merge(build_public_router(state.clone()))
        .layer(TraceLayer::new_for_http());

    // TCP router — dashboard/static assets are public, API routes require dashboard auth.
    let tcp_app = build_public_router(state.clone())
        .merge(
            build_api_router(state.clone()).layer(middleware::from_fn_with_state(
                state.clone(),
                auth::require_auth,
            )),
        )
        .layer(TraceLayer::new_for_http());

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

async fn rotate_dashboard_token(State(state): State<SharedState>) -> impl IntoResponse {
    match rotate_dashboard_token_inner(&state).await {
        Ok(token) => (StatusCode::OK, Json(serde_json::json!({ "token": token }))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

async fn get_version_status(State(state): State<SharedState>) -> impl IntoResponse {
    match version::resolve_version_status(&state.version_status).await {
        Ok(status) => (StatusCode::OK, Json(status)).into_response(),
        Err(error) => internal_error(error).into_response(),
    }
}

async fn rotate_dashboard_token_inner(state: &SharedState) -> anyhow::Result<String> {
    let current = state.dashboard_auth.read().await.clone();
    let (token, dashboard_auth) = issue_dashboard_token(Utc::now(), current.as_ref())?;

    {
        let mut auth = state.dashboard_auth.write().await;
        *auth = Some(dashboard_auth.clone());
    }

    let mut cfg = state.config.clone();
    cfg.dashboard_auth = Some(dashboard_auth);
    crate::config::save(&cfg)?;

    Ok(token)
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

const APP_OBJECT_STORE_ENV_KEYS: [&str; 15] = [
    "DEKU_OBJECT_STORE_PROVIDER",
    "DEKU_OBJECT_STORE_BUCKET",
    "DEKU_OBJECT_STORE_REGION",
    "DEKU_OBJECT_STORE_ENDPOINT",
    "DEKU_OBJECT_STORE_PREFIX",
    "DEKU_OBJECT_STORE_PATH_STYLE",
    "AWS_ACCESS_KEY_ID",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_REGION",
    "AWS_DEFAULT_REGION",
    "AWS_ENDPOINT_URL",
    "AWS_ENDPOINT_URL_S3",
    "S3_BUCKET",
    "S3_PREFIX",
    "S3_PATH_STYLE",
];

#[derive(Debug, Deserialize)]
struct ObjectStoreAppLinkBody {
    #[serde(default)]
    prefix: Option<String>,
}

fn app_object_store_env_pairs(
    object_store: &ObjectStoreConfig,
    prefix: &str,
) -> [(&'static str, String); 15] {
    let path_style = object_store.path_style.to_string();
    [
        (
            "DEKU_OBJECT_STORE_PROVIDER",
            object_store.provider.to_string(),
        ),
        ("DEKU_OBJECT_STORE_BUCKET", object_store.bucket.to_string()),
        ("DEKU_OBJECT_STORE_REGION", object_store.region.to_string()),
        (
            "DEKU_OBJECT_STORE_ENDPOINT",
            object_store.endpoint.to_string(),
        ),
        ("DEKU_OBJECT_STORE_PREFIX", prefix.to_string()),
        ("DEKU_OBJECT_STORE_PATH_STYLE", path_style.clone()),
        ("AWS_ACCESS_KEY_ID", object_store.access_key_id.to_string()),
        (
            "AWS_SECRET_ACCESS_KEY",
            object_store.secret_access_key.to_string(),
        ),
        ("AWS_REGION", object_store.region.to_string()),
        ("AWS_DEFAULT_REGION", object_store.region.to_string()),
        ("AWS_ENDPOINT_URL", object_store.endpoint.to_string()),
        ("AWS_ENDPOINT_URL_S3", object_store.endpoint.to_string()),
        ("S3_BUCKET", object_store.bucket.to_string()),
        ("S3_PREFIX", prefix.to_string()),
        ("S3_PATH_STYLE", path_style),
    ]
}

async fn get_app_object_store_link(
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

    let config_vars = match queries::get_config_vars(&state.pool, &app.id).await {
        Ok(vars) => vars,
        Err(e) => return internal_error(e).into_response(),
    };

    let values = config_vars
        .into_iter()
        .map(|entry| (entry.key, entry.value))
        .collect::<std::collections::HashMap<_, _>>();

    match crate::config::load() {
        Ok(cfg) => {
            let Some(object_store) = cfg.object_store else {
                return (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "app": app.name,
                        "configured": false,
                        "linked": APP_OBJECT_STORE_ENV_KEYS.iter().any(|key| values.contains_key(*key)),
                        "link": serde_json::Value::Null,
                    })),
                )
                    .into_response();
            };

            let default_prefix =
                crate::objectstore::normalized_app_prefix(&object_store, &app.name, None);
            let linked_keys = APP_OBJECT_STORE_ENV_KEYS
                .iter()
                .filter(|key| values.contains_key(**key))
                .map(|key| (*key).to_string())
                .collect::<Vec<_>>();
            let linked = !linked_keys.is_empty();
            let prefix = values
                .get("S3_PREFIX")
                .or_else(|| values.get("DEKU_OBJECT_STORE_PREFIX"))
                .cloned()
                .unwrap_or(default_prefix);
            let path_style = values
                .get("S3_PATH_STYLE")
                .or_else(|| values.get("DEKU_OBJECT_STORE_PATH_STYLE"))
                .and_then(|value| value.parse::<bool>().ok())
                .unwrap_or(object_store.path_style);

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "app": app.name,
                    "configured": true,
                    "linked": linked,
                    "link": {
                        "provider": values.get("DEKU_OBJECT_STORE_PROVIDER").cloned().unwrap_or_else(|| object_store.provider.clone()),
                        "bucket": values.get("S3_BUCKET").or_else(|| values.get("DEKU_OBJECT_STORE_BUCKET")).cloned().unwrap_or_else(|| object_store.bucket.clone()),
                        "region": values.get("AWS_REGION").or_else(|| values.get("DEKU_OBJECT_STORE_REGION")).cloned().unwrap_or_else(|| object_store.region.clone()),
                        "endpoint": values.get("AWS_ENDPOINT_URL").or_else(|| values.get("DEKU_OBJECT_STORE_ENDPOINT")).cloned().unwrap_or_else(|| object_store.endpoint.clone()),
                        "prefix": prefix,
                        "path_style": path_style,
                        "secret_present": values.get("AWS_SECRET_ACCESS_KEY").map(|value| !value.trim().is_empty()).unwrap_or(false),
                        "linked_keys": linked_keys,
                    }
                })),
            )
                .into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

async fn link_app_object_store(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(body): Json<ObjectStoreAppLinkBody>,
) -> impl IntoResponse {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(app) => app,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };

    let cfg = match crate::config::load() {
        Ok(cfg) => cfg,
        Err(e) => return internal_error(e).into_response(),
    };
    let Some(object_store) = cfg.object_store else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "object store is not configured" })),
        )
            .into_response();
    };

    let prefix =
        crate::objectstore::normalized_app_prefix(&object_store, &app.name, body.prefix.as_deref());

    for (key, value) in app_object_store_env_pairs(&object_store, &prefix) {
        if let Err(e) = queries::set_config_var(&state.pool, &app.id, key, &value, false).await {
            return internal_error(e).into_response();
        }
    }

    state.events.emit(
        Some(app.id),
        "app.objectstore.linked",
        Some(serde_json::json!({
            "app": app.name,
            "bucket": object_store.bucket,
            "prefix": prefix,
        })),
    );

    get_app_object_store_link(State(state), axum::extract::Path(name))
        .await
        .into_response()
}

async fn unlink_app_object_store(
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

    for key in APP_OBJECT_STORE_ENV_KEYS {
        if let Err(e) = queries::unset_config_var(&state.pool, &app.id, key).await {
            return internal_error(e).into_response();
        }
    }

    state.events.emit(
        Some(app.id),
        "app.objectstore.unlinked",
        Some(serde_json::json!({ "app": app.name })),
    );

    StatusCode::NO_CONTENT.into_response()
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
            state
                .plugins
                .run_app_create(&AppContext {
                    app: app.clone(),
                    data_dir: state.config.data_dir.clone(),
                })
                .await;
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
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(app) => app,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };

    match queries::delete_app(&state.pool, &name).await {
        Ok(()) => {
            if let Err(e) = crate::proxy::remove_app_config(&state.config.angie_conf_dir, &name) {
                tracing::warn!("failed to remove angie config for {name}: {e}");
            }
            if let Err(e) = crate::proxy::reload().await {
                tracing::warn!("angie reload failed after app delete: {e}");
            }
            state
                .plugins
                .run_app_destroy(&AppContext {
                    app,
                    data_dir: state.config.data_dir.clone(),
                })
                .await;
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

fn normalize_check_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        "/".to_string()
    } else if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

async fn run_live_http_probe(
    host_port: u16,
    path: &str,
    timeout_secs: u64,
) -> AppChecksProbeResult {
    let target = format!("http://127.0.0.1:{host_port}");
    let url = format!("{target}{path}");
    let started = std::time::Instant::now();

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        reqwest::get(&url),
    )
    .await;

    match result {
        Ok(Ok(response)) => {
            let status_code = response.status().as_u16();
            AppChecksProbeResult {
                ok: response.status().is_success() || status_code < 400,
                target,
                path: path.to_string(),
                status_code: Some(status_code),
                latency_ms: Some(started.elapsed().as_millis() as u64),
                error: None,
            }
        }
        Ok(Err(error)) => AppChecksProbeResult {
            ok: false,
            target,
            path: path.to_string(),
            status_code: None,
            latency_ms: Some(started.elapsed().as_millis() as u64),
            error: Some(error.to_string()),
        },
        Err(_) => AppChecksProbeResult {
            ok: false,
            target,
            path: path.to_string(),
            status_code: None,
            latency_ms: Some(started.elapsed().as_millis() as u64),
            error: Some(format!("timed out after {timeout_secs}s")),
        },
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
    let plugins = state.plugins.clone();
    let app_id = app.id.clone();

    // Run deploy in background so the HTTP response returns immediately
    tokio::spawn(async move {
        if let Err(e) =
            crate::deploy::run_deploy(&pool, &docker, &events, &cfg, plugins.as_ref(), req).await
        {
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
    let plugins = state.plugins.clone();
    let app_id = app.id.clone();
    let to_id = body.deployment_id.clone();

    tokio::spawn(async move {
        if let Err(e) = crate::deploy::rollback(
            &pool,
            &docker,
            &events,
            &cfg,
            plugins.as_ref(),
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

#[derive(Debug, Deserialize)]
struct AppChecksQuery {
    path: Option<String>,
    timeout_secs: Option<u64>,
}

fn default_log_lines() -> usize {
    100
}

#[derive(Debug, Serialize)]
struct AppChecksDeploymentSummary {
    id: String,
    status: deku_core::types::DeployStatus,
    builder: deku_core::types::BuilderType,
    image_tag: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct AppChecksContainerSummary {
    id: String,
    deployment_id: String,
    process_type: String,
    status: String,
    host_port: Option<i64>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct AppChecksProbeResult {
    ok: bool,
    target: String,
    path: String,
    status_code: Option<u16>,
    latency_ms: Option<u64>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct AppChecksResponse {
    app: String,
    status: String,
    latest_deployment: Option<AppChecksDeploymentSummary>,
    routing: RoutingAppStatus,
    containers: Vec<AppChecksContainerSummary>,
    port_mappings: Vec<deku_core::types::PortMapping>,
    probe: Option<AppChecksProbeResult>,
    issues: Vec<String>,
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

async fn get_app_checks(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Query(params): Query<AppChecksQuery>,
) -> impl IntoResponse {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(app) => app,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };

    let latest_deployment = match queries::get_latest_deployment(&state.pool, &app.id).await {
        Ok(deployment) => deployment.map(|deployment| AppChecksDeploymentSummary {
            id: deployment.id,
            status: deployment.status,
            builder: deployment.builder,
            image_tag: deployment.image_tag,
            created_at: deployment.created_at,
        }),
        Err(e) => return internal_error(e).into_response(),
    };

    let routing = match build_routing_status_for_app(&state, &app).await {
        Ok(routing) => routing,
        Err(e) => return internal_error(e).into_response(),
    };

    let containers = match queries::list_containers_for_app(&state.pool, &app.id).await {
        Ok(containers) => containers,
        Err(e) => return internal_error(e).into_response(),
    };

    let port_mappings = match queries::list_port_mappings(&state.pool, &app.id).await {
        Ok(ports) => ports,
        Err(e) => return internal_error(e).into_response(),
    };

    let path = normalize_check_path(params.path.as_deref().unwrap_or("/"));
    let timeout_secs = params.timeout_secs.unwrap_or(5);
    let probe = if let Some(port) = port_mappings.first() {
        Some(run_live_http_probe(port.host_port as u16, &path, timeout_secs).await)
    } else {
        None
    };

    let mut issues = Vec::new();
    if latest_deployment.is_none() {
        issues.push("no live deployment recorded".to_string());
    }
    if containers.is_empty() {
        issues.push("no running containers recorded".to_string());
    }
    if probe.is_none() {
        issues.push("no published web port available for an HTTP probe".to_string());
    }
    if let Some(probe) = &probe {
        if !probe.ok {
            issues.push(format!(
                "local HTTP probe failed for {}{}",
                probe.target, probe.path
            ));
        }
    }
    issues.extend(routing.issues.iter().cloned());

    let has_fatal_issue = latest_deployment.is_none()
        || containers.is_empty()
        || probe.as_ref().is_some_and(|probe| !probe.ok);

    let container_summaries = containers
        .into_iter()
        .map(|container| AppChecksContainerSummary {
            id: container.id,
            deployment_id: container.deployment_id,
            process_type: container.process_type,
            status: container.status,
            host_port: container.host_port,
            created_at: container.created_at,
        })
        .collect::<Vec<_>>();

    (
        StatusCode::OK,
        Json(serde_json::json!(AppChecksResponse {
            app: app.name,
            status: if has_fatal_issue {
                "fail".to_string()
            } else if issues.is_empty() {
                "pass".to_string()
            } else {
                "warn".to_string()
            },
            latest_deployment,
            routing,
            containers: container_summaries,
            port_mappings,
            probe,
            issues,
        })),
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
    let plugins = state.plugins.clone();
    let app_id = app.id.clone();

    tokio::spawn(async move {
        if let Err(e) =
            crate::deploy::run_deploy(&pool, &docker, &events, &cfg, plugins.as_ref(), req).await
        {
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
