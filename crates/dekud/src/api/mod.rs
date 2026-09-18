use std::convert::Infallible;
use std::future::IntoFuture;
use std::sync::Arc;

use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::{
    extract::{DefaultBodyLimit, Query, State},
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
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, UnixListener};
use tokio::sync::RwLock;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::config::{BuildHostConfig, DekuConfig, RegistryConfig};

use crate::container::DockerClient;
use crate::db::queries;
use crate::deploy::{DeployRequest, DeploySource};
use crate::events::EventSender;
use crate::version::{self, CachedVersionStatus};
use acme as acme_settings;
use deku_core::{
    auth::{issue_dashboard_token, DashboardTokenState},
    types::{NewApp, ObjectStoreConfig, Upstream},
};
use deku_plugin_sdk::context::AppContext;

mod acme;
pub mod auth;
mod console;
pub mod openapi;
mod services;

const ARCHIVE_UPLOAD_LIMIT: usize = 512 * 1024 * 1024;

pub struct AppState {
    pub config: DekuConfig,
    pub dashboard_auth: RwLock<Option<DashboardTokenState>>,
    pub version_status: RwLock<Option<CachedVersionStatus>>,
    pub pool: SqlitePool,
    pub events: EventSender,
    pub logs: std::sync::Arc<crate::logs::LogBus>,
    pub docker: DockerClient,
    pub plugins: std::sync::Arc<crate::plugins::PluginRegistry>,
    pub deploy_locks: crate::deploy_lock::AppDeployLocks,
}

impl AppState {
    pub fn new(
        config: DekuConfig,
        pool: SqlitePool,
        events: EventSender,
        logs: std::sync::Arc<crate::logs::LogBus>,
        docker: DockerClient,
        plugins: std::sync::Arc<crate::plugins::PluginRegistry>,
    ) -> Arc<Self> {
        Arc::new(Self {
            dashboard_auth: RwLock::new(config.dashboard_auth.clone()),
            version_status: RwLock::new(None),
            config,
            pool,
            events,
            logs,
            docker,
            plugins,
            deploy_locks: crate::deploy_lock::AppDeployLocks::default(),
        })
    }
}

pub type SharedState = Arc<AppState>;

fn build_api_router(state: SharedState) -> Router {
    Router::new()
        // Apps
        .route("/api/apps", get(list_apps).post(create_app))
        .route("/api/apps/{name}", get(get_app).delete(delete_app))
        .route("/api/apps/{name}/rename", post(rename_app))
        .route("/api/apps/{name}/clone", post(clone_app))
        .route(
            "/api/apps/{name}/auth",
            get(get_app_auth_handler)
                .post(set_app_auth_handler)
                .delete(delete_app_auth_handler),
        )
        .route(
            "/api/apps/{name}/maintenance",
            get(get_maintenance).post(set_maintenance),
        )
        .route(
            "/api/apps/{name}/redirects",
            get(list_redirects_handler).post(add_redirect_handler),
        )
        .route(
            "/api/apps/{name}/redirects/{id}",
            delete(remove_redirect_handler),
        )
        .route("/api/doctor", get(doctor))
        // Environments
        .route(
            "/api/apps/{name}/environments",
            get(list_environments_handler).post(create_environment_handler),
        )
        .route(
            "/api/apps/{name}/environments/{slug}",
            delete(delete_environment_handler),
        )
        // Deploy tokens
        .route(
            "/api/apps/{name}/deploy-tokens",
            get(list_app_deploy_tokens).post(create_app_deploy_token),
        )
        .route(
            "/api/apps/{name}/deploy-tokens/{id}",
            delete(revoke_app_deploy_token),
        )
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
        .route("/api/apps/{name}/logs/stream", get(stream_app_logs))
        .route("/api/apps/{name}/checks", get(get_app_checks))
        // One-off commands
        .route("/api/apps/{name}/run", post(console::run))
        .route("/api/apps/{name}/exec", post(console::exec))
        // Config vars
        .route("/api/apps/{name}/config", get(list_config).post(set_config))
        .route("/api/apps/{name}/config/{key}", delete(unset_config))
        .route("/api/apps/{name}/config/import", post(import_app_config))
        .route(
            "/api/apps/{name}/objectstore",
            get(get_app_object_store_link)
                .post(link_app_object_store)
                .delete(unlink_app_object_store),
        )
        // Process scale
        .route("/api/apps/{name}/ps", get(list_processes))
        .route("/api/apps/{name}/scale", get(get_scale).post(set_scale))
        .route("/api/apps/{name}/limits", get(get_limits).post(set_limits))
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
        .route("/api/plugins/runtime", get(get_plugins_runtime))
        .route("/api/plugins/{name}", delete(uninstall_plugin))
        // Object store
        .route(
            "/api/objectstore",
            get(get_object_store_config)
                .post(set_object_store_config)
                .delete(unset_object_store_config),
        )
        .route("/api/objectstore/test", post(test_object_store_config))
        // Build host and registry
        .route(
            "/api/build-host",
            get(get_build_host)
                .post(set_build_host)
                .delete(unset_build_host),
        )
        .route("/api/build-host/check", post(check_build_host))
        .route("/api/build-host/init", post(init_build_host))
        .route(
            "/api/registry",
            get(get_registry).post(set_registry).delete(unset_registry),
        )
        // Dashboard auth
        .route("/api/dashboard/session", post(verify_dashboard_session))
        .route("/api/dashboard/token", post(rotate_dashboard_token))
        .route("/api/version", get(get_version_status))
        // Archive deploy
        .route(
            "/api/apps/{name}/deploy/archive",
            post(deploy_archive).layer(DefaultBodyLimit::max(ARCHIVE_UPLOAD_LIMIT)),
        )
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
        .route(
            "/api/mysql/services/{name}/backups",
            get(services::my_backups).post(services::my_backup),
        )
        .route(
            "/api/mysql/services/{name}/restore/{backup_id}",
            post(services::my_restore),
        )
        // Generic managed-service routes (per-type routes above are aliases)
        .route(
            "/api/services/{type}",
            get(services::svc_list).post(services::svc_create),
        )
        .route(
            "/api/services/{type}/{name}",
            get(services::svc_info).delete(services::svc_destroy),
        )
        .route(
            "/api/services/{type}/{name}/link/{app}",
            post(services::svc_link).delete(services::svc_unlink),
        )
        .route("/api/services/{type}/{name}/logs", get(services::svc_logs))
        .route(
            "/api/services/{type}/{name}/backups",
            get(services::svc_backups).post(services::svc_backup),
        )
        .route(
            "/api/services/{type}/{name}/restore/{backup_id}",
            post(services::svc_restore),
        )
        // Backup schedules (any managed service)
        .route(
            "/api/backup-schedules",
            get(services::list_backup_schedules),
        )
        .route(
            "/api/services/{name}/backup-schedule",
            get(services::get_backup_schedule)
                .post(services::set_backup_schedule)
                .delete(services::delete_backup_schedule),
        )
        // Alerts and metrics
        .route("/api/alerts", get(list_alerts_handler))
        .route("/api/metrics", get(metrics_handler))
        // Letsencrypt
        .route("/api/letsencrypt/enable/{app}", post(services::le_enable))
        .route("/api/letsencrypt/disable/{app}", post(services::le_disable))
        .route("/api/letsencrypt/status/{app}", get(services::le_status))
        .route(
            "/api/letsencrypt/config",
            get(services::le_get_config).post(services::le_config),
        )
        // Automatic certificates
        .route(
            "/api/acme",
            get(acme_settings::get_acme).put(acme_settings::put_acme),
        )
        .route("/api/acme/verify", post(acme_settings::verify_acme))
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
    let router = Router::new()
        .route("/healthz", get(health_check))
        .route("/api/openapi.json", get(openapi::spec))
        .route("/api/docs", get(openapi::docs));

    if crate::config::dashboard_assets_available(&state.config) {
        router
            .fallback_service(ServeDir::new(dashboard_dir).append_index_html_on_directories(true))
            .with_state(state)
    } else {
        router.fallback(missing_dashboard_page).with_state(state)
    }
}

/// Routes only reachable over the daemon's Unix socket.
///
/// Angie calls these from the host while answering an ACME challenge. They are
/// absent from the TCP API and from the OpenAPI document on purpose: nothing
/// off-host should be able to ask the daemon to write DNS records.
///
/// Defined after `build_public_router` because the OpenAPI coverage guard
/// scans the router declared between `build_api_router` and it.
fn build_internal_router(state: SharedState) -> Router {
    Router::new()
        .route("/internal/acme/dns-hook", post(acme_dns_hook))
        .with_state(state)
}

/// Answer an `acme_hook` callback from Angie during a DNS-01 challenge.
///
/// Only the DNS challenge is handled: a wildcard certificate requires it, and
/// the other methods never need provider access.
async fn acme_dns_hook(
    State(state): State<SharedState>,
    headers: axum::http::HeaderMap,
) -> Response {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    };

    let challenge = header("x-deku-acme-challenge").unwrap_or_default();
    if challenge != "dns" {
        return bad_request(format!(
            "unsupported ACME challenge '{challenge}'; only dns is handled"
        ))
        .into_response();
    }

    let action = match header("x-deku-acme-hook")
        .as_deref()
        .map(crate::acme::ChallengeAction::parse)
    {
        Some(Ok(action)) => action,
        Some(Err(error)) => return bad_request(error.to_string()).into_response(),
        None => return bad_request("missing x-deku-acme-hook header").into_response(),
    };

    let Some(domain) = header("x-deku-acme-domain") else {
        return bad_request("missing x-deku-acme-domain header").into_response();
    };
    let Some(keyauth) = header("x-deku-acme-keyauth") else {
        return bad_request("missing x-deku-acme-keyauth header").into_response();
    };

    // Read the config fresh: a token or provider saved from the dashboard takes
    // effect without restarting the daemon.
    let cfg = current_config(&state);

    // Anything that can write to the socket can reach this, so the domain is
    // checked before any record is touched.
    if let Err(error) = crate::acme::authorize_domain(cfg.global_domain.as_deref(), &domain) {
        tracing::warn!(%domain, "rejected an ACME challenge: {error}");
        return bad_request(error.to_string()).into_response();
    }

    let client = match crate::acme::provider_client(&cfg) {
        Ok(Some(client)) => client,
        Ok(None) => return bad_request("ACME is not enabled").into_response(),
        Err(error) => return internal_error(error).into_response(),
    };

    let Some(zone) = cfg.global_domain.as_deref() else {
        return bad_request("no global_domain is configured").into_response();
    };

    let zone_id = match client.zone_id(zone).await {
        Ok(zone_id) => zone_id,
        Err(error) => {
            tracing::error!(%domain, "ACME challenge could not resolve its zone: {error}");
            return internal_error(error).into_response();
        }
    };

    match crate::acme::apply_challenge(&client, &zone_id, &domain, action, &keyauth).await {
        Ok(()) => {
            tracing::info!(%domain, ?action, "answered an ACME challenge");
            StatusCode::OK.into_response()
        }
        Err(error) => {
            tracing::error!(%domain, "ACME challenge failed: {error}");
            internal_error(error).into_response()
        }
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
    let make_span = |request: &axum::http::Request<axum::body::Body>| {
        tracing::info_span!(
            "http_request",
            method = %request.method(),
            path = %request.uri().path(),
        )
    };

    // Unix socket router — trusted local access, no auth required
    let unix_app = build_api_router(state.clone())
        .merge(build_internal_router(state.clone()))
        .layer(middleware::from_fn(security_headers))
        .merge(build_public_router(state.clone()))
        .layer(TraceLayer::new_for_http().make_span_with(make_span));

    // TCP router — dashboard/static assets are public, API routes require dashboard auth.
    let tcp_api = build_api_router(state.clone())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ))
        .layer(middleware::from_fn(security_headers));
    let tcp_app = build_public_router(state.clone())
        .merge(tcp_api)
        .layer(TraceLayer::new_for_http().make_span_with(make_span));

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
        axum::serve(
            tcp_listener,
            tcp_app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .into_future(),
        axum::serve(unix_listener, unix_app).into_future(),
    )?;

    Ok(())
}

/// Force no-referrer and no-store on API responses so a token carried in a URL
/// is not propagated to other origins or retained by caches.
async fn security_headers(request: axum::extract::Request, next: middleware::Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        axum::http::header::REFERRER_POLICY,
        axum::http::HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn internal_error(e: impl std::fmt::Display) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": e.to_string() })),
    )
}

fn multipart_error(error: axum::extract::multipart::MultipartError) -> Response {
    let status = error.status();
    if status == StatusCode::PAYLOAD_TOO_LARGE {
        return (
            status,
            Json(serde_json::json!({
                "error": format!(
                    "archive exceeds the {} MiB upload limit",
                    ARCHIVE_UPLOAD_LIMIT / (1024 * 1024)
                )
            })),
        )
            .into_response();
    }

    (
        status,
        Json(serde_json::json!({ "error": error.body_text() })),
    )
        .into_response()
}

fn not_found(msg: impl std::fmt::Display) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": msg.to_string() })),
    )
}

fn bad_request(msg: impl std::fmt::Display) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": msg.to_string() })),
    )
}

// ── Health ────────────────────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/healthz",
    tag = "system",
    responses((status = 200, description = "Daemon health", body = openapi::HealthSchema))
)]
async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok", "service": "dekud" }))
}

#[utoipa::path(
    post,
    path = "/api/dashboard/token",
    tag = "dashboard",
    responses((status = 200, description = "Rotate the dashboard token"))
)]
async fn rotate_dashboard_token(State(state): State<SharedState>) -> impl IntoResponse {
    match rotate_dashboard_token_inner(&state).await {
        Ok(token) => (StatusCode::OK, Json(serde_json::json!({ "token": token }))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/dashboard/session",
    tag = "dashboard",
    responses((status = 204, description = "No content"))
)]
async fn verify_dashboard_session() -> impl IntoResponse {
    auth::log_dashboard_session_verified();
    StatusCode::NO_CONTENT
}

#[utoipa::path(
    get,
    path = "/api/version",
    tag = "system",
    responses((status = 200, description = "Version and update status"))
)]
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

    // Read the settings that can change while the daemon runs rather than
    // saving the snapshot from startup: this write replaces the whole file, so
    // anything the snapshot predates would be silently undone — an ACME client
    // the dashboard had just configured among it.
    let mut cfg = current_config(state);
    cfg.dashboard_auth = Some(dashboard_auth);
    crate::config::save(&cfg)?;

    Ok(token)
}

// ── Object Store ──────────────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/objectstore",
    tag = "object-store",
    responses((status = 200, description = "Show object-store configuration"))
)]
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

#[utoipa::path(
    post,
    path = "/api/objectstore",
    tag = "object-store",
    request_body = openapi::ObjectStoreConfigSchema,
    responses((status = 200, description = "Configure the object store"))
)]
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

#[utoipa::path(
    delete,
    path = "/api/objectstore",
    tag = "object-store",
    responses((status = 204, description = "No content"))
)]
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

#[utoipa::path(
    post,
    path = "/api/objectstore/test",
    tag = "object-store",
    responses((status = 200, description = "Test object-store connectivity"))
)]
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

#[utoipa::path(
    post,
    path = "/api/apps/{name}/config/import",
    tag = "config",
    params(("name" = String, Path, description = "App name")),
    request_body = ImportConfigBody,
    responses(
        (status = 200, description = "Import summary", body = openapi::ConfigImportSummarySchema),
        (status = 400, description = "Invalid key")
    )
)]
async fn import_app_config(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(body): Json<ImportConfigBody>,
) -> Response {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(app) => app,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };

    // Only the key names matter here, so read the stored rows directly.
    let existing: std::collections::HashSet<String> =
        match queries::get_config_vars_raw(&state.pool, &app.id).await {
            Ok(vars) => vars.into_iter().map(|var| var.key).collect(),
            Err(e) => return internal_error(e).into_response(),
        };

    let mut created = 0u64;
    let mut overwritten = 0u64;
    let mut skipped: Vec<String> = Vec::new();

    for (key, value) in &body.vars {
        if key.trim().is_empty() || key.len() > 256 {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("invalid key '{key}'") })),
            )
                .into_response();
        }

        if existing.contains(key) {
            if !body.overwrite {
                skipped.push(key.clone());
                continue;
            }
            overwritten += 1;
        } else {
            created += 1;
        }

        if let Err(e) =
            crate::secrets::set_config_var(&state.pool, &state.config, &app.id, key, value, false)
                .await
        {
            return internal_error(e).into_response();
        }
    }

    state.events.emit(
        Some(app.id.clone()),
        "app.config.imported",
        Some(serde_json::json!({
            "created": created,
            "overwritten": overwritten,
            "skipped": skipped,
        })),
    );

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "created": created,
            "overwritten": overwritten,
            "skipped": skipped,
        })),
    )
        .into_response()
}

// ── Rename and clone ──────────────────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/api/apps/{name}/rename",
    tag = "apps",
    params(("name" = String, Path, description = "App name")),
    request_body = RenameAppBody,
    responses(
        (status = 200, description = "App renamed", body = openapi::AppSchema),
        (status = 400, description = "Invalid name"),
        (status = 409, description = "Name already taken")
    )
)]
async fn rename_app(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(body): Json<RenameAppBody>,
) -> Response {
    let new_name = body.name.trim().to_string();
    if let Err(message) = crate::app_name::validate(&new_name) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": message })),
        )
            .into_response();
    }

    let app = match queries::get_app(&state.pool, &name).await {
        Ok(app) => app,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };

    if new_name == app.name {
        return (StatusCode::OK, Json(serde_json::json!(app))).into_response();
    }

    if let Err(e) = queries::rename_app(&state.pool, &app.id, &new_name).await {
        return match e {
            deku_core::error::DekuError::AppAlreadyExists(taken) => (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "error": format!("app '{taken}' already exists"),
                })),
            )
                .into_response(),
            other => internal_error(other).into_response(),
        };
    }

    // The vhost file is named after the app; container routing is by published
    // port, so the running containers keep serving across the rename.
    if let Err(error) = crate::proxy::remove_app_config(&state.config.angie_conf_dir, &name) {
        tracing::warn!("failed to remove angie config for {name}: {error}");
    }
    if let Err(error) = reconcile_proxy_for_app(&state, &app.id, &new_name).await {
        tracing::warn!("failed to write angie config for {new_name}: {error}");
    }
    if let Err(error) = crate::proxy::reload().await {
        tracing::warn!("angie reload failed after rename: {error}");
    }

    state.events.emit(
        Some(app.id.clone()),
        "app.renamed",
        Some(serde_json::json!({ "from": name, "to": new_name })),
    );

    match queries::get_app(&state.pool, &new_name).await {
        Ok(renamed) => (StatusCode::OK, Json(serde_json::json!(renamed))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/apps/{name}/clone",
    tag = "apps",
    params(("name" = String, Path, description = "App name")),
    request_body = RenameAppBody,
    responses(
        (status = 201, description = "App cloned; the response lists copied and skipped settings"),
        (status = 400, description = "Invalid name"),
        (status = 409, description = "Name already taken")
    )
)]
async fn clone_app(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(body): Json<RenameAppBody>,
) -> Response {
    let new_name = body.name.trim().to_string();
    if let Err(message) = crate::app_name::validate(&new_name) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": message })),
        )
            .into_response();
    }

    let source = match queries::get_app(&state.pool, &name).await {
        Ok(app) => app,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };

    let created = match queries::create_app(
        &state.pool,
        &deku_core::types::NewApp {
            name: new_name.clone(),
        },
    )
    .await
    {
        Ok(app) => app,
        Err(deku_core::error::DekuError::AppAlreadyExists(_)) => {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "error": format!("app '{new_name}' already exists"),
                })),
            )
                .into_response()
        }
        Err(e) => return internal_error(e).into_response(),
    };

    let summary = match queries::clone_app_settings(&state.pool, &source.id, &created.id).await {
        Ok(summary) => summary,
        Err(e) => return internal_error(e).into_response(),
    };

    state.events.emit(
        Some(created.id.clone()),
        "app.created",
        Some(serde_json::json!({ "name": created.name, "cloned_from": source.name })),
    );
    if let Err(error) = crate::hooks::fire(
        &current_config(&state),
        crate::hooks::HookEvent::AppCreated,
        crate::hooks::HookEventData {
            app: &created,
            deployment: None,
            detail: serde_json::json!({ "cloned_from": source.name }),
        },
    )
    .await
    {
        tracing::warn!("app.created hook reported an error: {error}");
    }

    (
        StatusCode::CREATED,
        Json(serde_json::json!({ "app": created, "copied": summary })),
    )
        .into_response()
}

// ── Deploy tokens ─────────────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/apps/{name}/deploy-tokens",
    tag = "deploy",
    params(("name" = String, Path, description = "App name")),
    responses((status = 200, description = "Deploy tokens", body = [openapi::DeployTokenSchema]))
)]
async fn list_app_deploy_tokens(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Response {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(app) => app,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };

    match queries::list_deploy_tokens(&state.pool, &app.id).await {
        Ok(tokens) => (
            StatusCode::OK,
            Json(serde_json::json!(tokens
                .iter()
                .map(deploy_token_json)
                .collect::<Vec<_>>())),
        )
            .into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/apps/{name}/deploy-tokens",
    tag = "deploy",
    params(("name" = String, Path, description = "App name")),
    request_body = CreateDeployTokenBody,
    responses(
        (status = 201, description = "Deploy token created; the token is shown once", body = openapi::NewDeployTokenSchema),
        (status = 400, description = "Invalid name")
    )
)]
async fn create_app_deploy_token(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(body): Json<CreateDeployTokenBody>,
) -> Response {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(app) => app,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };

    let label = body.name.trim();
    if label.is_empty() || label.len() > 64 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "token name must be 1-64 characters" })),
        )
            .into_response();
    }

    match crate::services::deploy_token::create(&state.pool, &app.id, label).await {
        Ok((token, plaintext)) => {
            let mut payload = deploy_token_json(&token);
            payload["token"] = serde_json::Value::String(plaintext);
            (StatusCode::CREATED, Json(payload)).into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/api/apps/{name}/deploy-tokens/{id}",
    tag = "deploy",
    params(
        ("name" = String, Path, description = "App name"),
        ("id" = String, Path, description = "Deploy token id")
    ),
    responses(
        (status = 204, description = "Deploy token revoked"),
        (status = 404, description = "App or token not found")
    )
)]
async fn revoke_app_deploy_token(
    State(state): State<SharedState>,
    axum::extract::Path((name, id)): axum::extract::Path<(String, String)>,
) -> Response {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(app) => app,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };

    match queries::delete_deploy_token(&state.pool, &app.id, &id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => not_found(format!("deploy token '{id}' not found")).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

// ── Build host and registry ───────────────────────────────────────────────────

/// Reload the fields managed through the config API (object store, registry,
/// build host) so changes take effect without restarting `dekud`. Paths, ports,
/// and other startup settings stay frozen at their running values.
fn current_config(state: &SharedState) -> DekuConfig {
    match crate::config::load() {
        Ok(fresh) => {
            let mut cfg = state.config.clone();
            cfg.object_store = fresh.object_store;
            cfg.registry = fresh.registry;
            cfg.build_host = fresh.build_host;
            cfg.hooks = fresh.hooks;
            cfg.acme = fresh.acme;
            cfg
        }
        Err(_) => state.config.clone(),
    }
}

#[utoipa::path(
    get,
    path = "/api/build-host",
    tag = "build-host",
    responses((status = 200, description = "Build host and registry status", body = openapi::BuildHostStatusSchema))
)]
async fn get_build_host() -> impl IntoResponse {
    match crate::config::load() {
        Ok(cfg) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "configured": cfg.build_host.is_some(),
                "build_host": cfg.build_host,
                "registry": cfg.registry.map(|registry| registry.redacted()),
            })),
        )
            .into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/build-host",
    tag = "build-host",
request_body = openapi::BuildHostSchema,
    responses((status = 200, description = "Build host saved"), (status = 400, description = "Invalid build host"))
)]
async fn set_build_host(Json(body): Json<BuildHostConfig>) -> impl IntoResponse {
    if body.host.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "build host 'host' cannot be empty" })),
        )
            .into_response();
    }

    match crate::config::load().and_then(|mut cfg| {
        cfg.build_host = Some(body.clone());
        crate::config::save(&cfg)?;
        Ok(())
    }) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({ "configured": true, "build_host": body })),
        )
            .into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/api/build-host",
    tag = "build-host",
    responses((status = 204, description = "Build host removed"))
)]
async fn unset_build_host() -> impl IntoResponse {
    match crate::config::load().and_then(|mut cfg| {
        cfg.build_host = None;
        crate::config::save(&cfg)?;
        Ok(())
    }) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/build-host/check",
    tag = "build-host",
    responses((status = 200, description = "Build host checks", body = openapi::BuildHostCheckSchema))
)]
async fn check_build_host() -> impl IntoResponse {
    match crate::config::load() {
        Ok(cfg) => match crate::build_remote::check_build_host(&cfg).await {
            Ok(checks) => {
                let ok = checks.iter().all(|check| check.status == "ok");
                let payload: Vec<serde_json::Value> = checks
                    .iter()
                    .map(|check| {
                        serde_json::json!({
                            "name": check.name,
                            "status": check.status,
                            "detail": check.detail,
                        })
                    })
                    .collect();
                (
                    StatusCode::OK,
                    Json(serde_json::json!({ "ok": ok, "checks": payload })),
                )
                    .into_response()
            }
            Err(e) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response(),
        },
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/build-host/init",
    tag = "build-host",
    responses((status = 200, description = "BuildKit initialization notes", body = openapi::BuildHostInitSchema))
)]
async fn init_build_host() -> impl IntoResponse {
    match crate::config::load() {
        Ok(cfg) => match crate::build_remote::init_build_host(&cfg).await {
            Ok(notes) => (
                StatusCode::OK,
                Json(serde_json::json!({ "ok": true, "notes": notes })),
            )
                .into_response(),
            Err(e) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response(),
        },
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/registry",
    tag = "registry",
    responses((status = 200, description = "Registry status", body = openapi::RegistryStatusSchema))
)]
async fn get_registry() -> impl IntoResponse {
    match crate::config::load() {
        Ok(cfg) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "configured": cfg.registry.is_some(),
                "registry": cfg.registry.map(|registry| registry.redacted()),
            })),
        )
            .into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/registry",
    tag = "registry",
request_body = openapi::RegistrySchema,
    responses((status = 200, description = "Registry saved"), (status = 400, description = "Invalid registry"))
)]
async fn set_registry(Json(mut body): Json<RegistryConfig>) -> impl IntoResponse {
    if body.server.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "registry 'server' cannot be empty" })),
        )
            .into_response();
    }

    let existing = crate::config::load().ok().and_then(|cfg| cfg.registry);
    // A redacted password round-tripped from `GET /api/registry` keeps the stored secret.
    if body.password.as_deref() == Some("********") {
        body.password = existing
            .as_ref()
            .and_then(|registry| registry.password.clone());
    }

    let response_body = body.redacted();
    match crate::config::load().and_then(|mut cfg| {
        cfg.registry = Some(body);
        crate::config::save(&cfg)?;
        Ok(())
    }) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({ "configured": true, "registry": response_body })),
        )
            .into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/api/registry",
    tag = "registry",
    responses((status = 204, description = "Registry removed"))
)]
async fn unset_registry() -> impl IntoResponse {
    match crate::config::load().and_then(|mut cfg| {
        cfg.registry = None;
        crate::config::save(&cfg)?;
        Ok(())
    }) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
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

#[derive(Debug, Deserialize, utoipa::ToSchema)]
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

#[utoipa::path(
    get,
    path = "/api/apps/{name}/objectstore",
    tag = "object-store",
    params(("name" = String, Path, description = "App name")),
    responses((status = 200, description = "Show the object-store link for an app"))
)]
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

    let config_vars =
        match crate::secrets::get_config_vars(&state.pool, &state.config, &app.id).await {
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

#[utoipa::path(
    post,
    path = "/api/apps/{name}/objectstore",
    tag = "object-store",
    params(("name" = String, Path, description = "App name")),
    request_body = ObjectStoreAppLinkBody,
    responses((status = 200, description = "Link object-store credentials into an app"))
)]
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
        if let Err(e) =
            crate::secrets::set_config_var(&state.pool, &state.config, &app.id, key, &value, false)
                .await
        {
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

#[utoipa::path(
    delete,
    path = "/api/apps/{name}/objectstore",
    tag = "object-store",
    params(("name" = String, Path, description = "App name")),
    responses((status = 204, description = "No content"))
)]
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

#[utoipa::path(
    get,
    path = "/api/apps",
    tag = "apps",
    responses((status = 200, description = "List apps", body = [openapi::AppSchema]))
)]
async fn list_apps(State(state): State<SharedState>) -> impl IntoResponse {
    match queries::list_apps(&state.pool).await {
        Ok(apps) => (StatusCode::OK, Json(serde_json::json!(apps))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/apps",
    tag = "apps",
    request_body = openapi::NewAppSchema,
    responses((status = 201, description = "App created", body = openapi::AppSchema))
)]
async fn create_app(
    State(state): State<SharedState>,
    Json(body): Json<NewApp>,
) -> impl IntoResponse {
    // An app name becomes a file name and a git remote path, so reject anything
    // that could escape a directory or look like a flag.
    if let Err(message) = crate::app_name::validate(body.name.trim()) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": message })),
        )
            .into_response();
    }

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
            if let Err(error) = crate::hooks::fire(
                &current_config(&state),
                crate::hooks::HookEvent::AppCreated,
                crate::hooks::HookEventData {
                    app: &app,
                    deployment: None,
                    detail: serde_json::json!({}),
                },
            )
            .await
            {
                tracing::warn!("app.created hook reported an error: {error}");
            }
            (StatusCode::CREATED, Json(serde_json::json!(app))).into_response()
        }
        Err(e) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/apps/{name}",
    tag = "apps",
    params(("name" = String, Path, description = "App name")),
    responses(
        (status = 200, description = "App detail", body = openapi::AppSchema),
        (status = 404, description = "App not found")
    )
)]
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

#[utoipa::path(
    delete,
    path = "/api/apps/{name}",
    tag = "apps",
    params(("name" = String, Path, description = "App name")),
    responses(
        (status = 204, description = "App deleted"),
        (status = 404, description = "App not found")
    )
)]
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
                    app: app.clone(),
                    data_dir: state.config.data_dir.clone(),
                })
                .await;
            if let Err(error) = crate::hooks::fire(
                &current_config(&state),
                crate::hooks::HookEvent::AppDestroyed,
                crate::hooks::HookEventData {
                    app: &app,
                    deployment: None,
                    detail: serde_json::json!({}),
                },
            )
            .await
            {
                tracing::warn!("app.destroyed hook reported an error: {error}");
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

#[derive(Debug, Deserialize)]
struct SetAppAuthBody {
    mode: String,
    username: Option<String>,
    password: Option<String>,
    forward_url: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/apps/{name}/auth",
    tag = "auth",
    params(("name" = String, Path, description = "App name")),
    responses((status = 200, description = "Auth status", body = openapi::AppAuthStatusSchema))
)]
async fn get_app_auth_handler(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Response {
    match crate::services::app_auth::status(&state.pool, &name).await {
        Ok(Some(record)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "configured": true,
                "mode": record.mode,
                "username": record.username,
                "forward_url": record.forward_url,
            })),
        )
            .into_response(),
        Ok(None) => (
            StatusCode::OK,
            Json(serde_json::json!({ "configured": false })),
        )
            .into_response(),
        Err(_) => not_found(format!("app '{name}' not found")).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/apps/{name}/auth",
    tag = "auth",
    params(("name" = String, Path, description = "App name")),
request_body = openapi::SetAppAuthSchema,
    responses((status = 204, description = "Auth updated"), (status = 400, description = "Invalid auth configuration"))
)]
async fn set_app_auth_handler(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(body): Json<SetAppAuthBody>,
) -> Response {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(app) => app,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(error) => return internal_error(error).into_response(),
    };

    let result = match body.mode.as_str() {
        "basic" => {
            crate::services::app_auth::enable_basic(
                &state.pool,
                &name,
                body.username.as_deref().unwrap_or_default(),
                body.password.as_deref().unwrap_or_default(),
            )
            .await
        }
        "forward" => {
            crate::services::app_auth::enable_forward(
                &state.pool,
                &name,
                body.forward_url.as_deref().unwrap_or_default(),
            )
            .await
        }
        other => Err(anyhow::anyhow!(
            "unknown auth mode '{other}'; expected 'basic' or 'forward'"
        )),
    };

    if let Err(error) = result {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": error.to_string() })),
        )
            .into_response();
    }

    if let Err(error) = reconcile_proxy_for_app(&state, &app.id, &name).await {
        return internal_error(error).into_response();
    }
    StatusCode::NO_CONTENT.into_response()
}

#[utoipa::path(
    delete,
    path = "/api/apps/{name}/auth",
    tag = "auth",
    params(("name" = String, Path, description = "App name")),
    responses((status = 204, description = "Auth removed"))
)]
async fn delete_app_auth_handler(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Response {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(app) => app,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(error) => return internal_error(error).into_response(),
    };

    if let Err(error) = crate::services::app_auth::disable(&state.pool, &name).await {
        return internal_error(error).into_response();
    }
    if let Err(error) = reconcile_proxy_for_app(&state, &app.id, &name).await {
        return internal_error(error).into_response();
    }
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct SetMaintenanceBody {
    enabled: bool,
    message: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/apps/{name}/maintenance",
    tag = "maintenance",
    params(("name" = String, Path, description = "App name")),
    responses((status = 200, description = "Maintenance status", body = openapi::MaintenanceSchema))
)]
async fn get_maintenance(
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
    match queries::get_app_maintenance(&state.pool, &app.id).await {
        Ok((enabled, message)) => (
            StatusCode::OK,
            Json(serde_json::json!({ "enabled": enabled, "message": message })),
        )
            .into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/apps/{name}/maintenance",
    tag = "maintenance",
    params(("name" = String, Path, description = "App name")),
request_body = openapi::MaintenanceSchema,
    responses((status = 204, description = "Maintenance updated"))
)]
async fn set_maintenance(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(body): Json<SetMaintenanceBody>,
) -> Response {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(app) => app,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };
    if body
        .message
        .as_deref()
        .is_some_and(|message| message.len() > 500)
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "maintenance message must be 500 characters or fewer" })),
        )
            .into_response();
    }
    let message = body
        .message
        .as_deref()
        .filter(|message| !message.is_empty());
    if let Err(e) = queries::set_app_maintenance(&state.pool, &app.id, body.enabled, message).await
    {
        return internal_error(e).into_response();
    }
    if let Err(e) = reconcile_proxy_for_app(&state, &app.id, &name).await {
        return internal_error(e).into_response();
    }
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct AddRedirectBody {
    source_path: String,
    target: String,
    code: Option<i64>,
}

#[utoipa::path(
    get,
    path = "/api/apps/{name}/redirects",
    tag = "redirects",
    params(("name" = String, Path, description = "App name")),
    responses((status = 200, description = "Redirects", body = [openapi::RedirectSchema]))
)]
async fn list_redirects_handler(
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
    match queries::list_redirects(&state.pool, &app.id).await {
        Ok(redirects) => (StatusCode::OK, Json(serde_json::json!(redirects))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/apps/{name}/redirects",
    tag = "redirects",
    params(("name" = String, Path, description = "App name")),
request_body = openapi::AddRedirectSchema,
    responses((status = 201, description = "Redirect created", body = openapi::RedirectSchema))
)]
async fn add_redirect_handler(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(body): Json<AddRedirectBody>,
) -> Response {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(app) => app,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };

    let source_path = body.source_path.trim();
    let target = body.target.trim();
    let code = body.code.unwrap_or(302);
    if !source_path.starts_with('/') || source_path.contains(char::is_whitespace) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "source_path must start with '/' and contain no spaces" })),
        )
            .into_response();
    }
    let target_ok =
        target.starts_with("http://") || target.starts_with("https://") || target.starts_with('/');
    if !target_ok {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "target must be an absolute http(s) URL or a path starting with '/'" })),
        )
            .into_response();
    }
    if !matches!(code, 301 | 302 | 307 | 308) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "code must be one of 301, 302, 307, 308" })),
        )
            .into_response();
    }

    let redirect = match queries::add_redirect(&state.pool, &app.id, source_path, target, code)
        .await
    {
        Ok(redirect) => redirect,
        Err(deku_core::error::DekuError::Database(sqlx::Error::Database(error)))
            if error.is_unique_violation() =>
        {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({ "error": format!("a redirect for '{source_path}' already exists") })),
            )
                .into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };
    if let Err(e) = reconcile_proxy_for_app(&state, &app.id, &name).await {
        return internal_error(e).into_response();
    }
    (StatusCode::CREATED, Json(serde_json::json!(redirect))).into_response()
}

#[utoipa::path(
    delete,
    path = "/api/apps/{name}/redirects/{id}",
    tag = "redirects",
    params(("name" = String, Path, description = "App name"), ("id" = String, Path, description = "Redirect id")),
    responses((status = 204, description = "Redirect removed"))
)]
async fn remove_redirect_handler(
    State(state): State<SharedState>,
    axum::extract::Path((name, id)): axum::extract::Path<(String, String)>,
) -> Response {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(app) => app,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };
    match queries::remove_redirect(&state.pool, &app.id, &id).await {
        Ok(()) => {}
        Err(e) => return internal_error(e).into_response(),
    }
    if let Err(e) = reconcile_proxy_for_app(&state, &app.id, &name).await {
        return internal_error(e).into_response();
    }
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct CreateEnvironmentBody {
    name: String,
    /// Defaults to a slug derived from `name`.
    slug: Option<String>,
    /// Git ref this environment tracks. Metadata only: nothing auto-deploys on
    /// push yet.
    branch: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/apps/{name}/environments",
    tag = "apps",
    params(("name" = String, Path, description = "App name")),
    responses((status = 200, description = "List an app's environments"))
)]
async fn list_environments_handler(
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

    match queries::list_environments(&state.pool, &app.id).await {
        Ok(environments) => (StatusCode::OK, Json(serde_json::json!(environments))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/apps/{name}/environments",
    tag = "apps",
    params(("name" = String, Path, description = "App name")),
    request_body = CreateEnvironmentBody,
    responses((status = 201, description = "Environment created"), (status = 400, description = "Invalid name or slug"))
)]
async fn create_environment_handler(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(body): Json<CreateEnvironmentBody>,
) -> impl IntoResponse {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(app) => app,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };

    let display_name = body.name.trim().to_string();
    if display_name.is_empty() {
        return bad_request("environment name must not be empty".to_string()).into_response();
    }
    // An explicit slug is taken as given and validated, so the caller gets the
    // slug they asked for or an error. A derived slug is normalized from the name.
    let slug = match body
        .slug
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(slug) => slug.to_string(),
        None => crate::app_name::slugify(&display_name),
    };
    if let Err(error) = crate::app_name::validate_slug(&slug) {
        return bad_request(error).into_response();
    }
    if slug == queries::PRODUCTION_ENVIRONMENT_SLUG {
        return bad_request(
            "production already exists for every app and cannot be recreated".to_string(),
        )
        .into_response();
    }

    let branch = body
        .branch
        .as_deref()
        .map(str::trim)
        .filter(|branch| !branch.is_empty());

    match queries::create_environment(&state.pool, &app.id, &display_name, &slug, branch, false)
        .await
    {
        Ok(environment) => {
            state.events.emit(
                Some(app.id.clone()),
                "app.environment.created",
                Some(serde_json::json!({ "slug": environment.slug })),
            );
            (StatusCode::CREATED, Json(serde_json::json!(environment))).into_response()
        }
        Err(deku_core::error::DekuError::Database(sqlx::Error::Database(error)))
            if error.is_unique_violation() =>
        {
            bad_request(format!("environment '{slug}' already exists")).into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/api/apps/{name}/environments/{slug}",
    tag = "apps",
    params(("name" = String, Path, description = "App name"), ("slug" = String, Path, description = "Environment slug")),
    responses((status = 204, description = "Environment removed"), (status = 400, description = "Production cannot be removed"))
)]
async fn delete_environment_handler(
    State(state): State<SharedState>,
    axum::extract::Path((name, slug)): axum::extract::Path<(String, String)>,
) -> impl IntoResponse {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(app) => app,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };

    match queries::delete_environment(&state.pool, &app.id, &slug).await {
        Ok(()) => {
            state.events.emit(
                Some(app.id.clone()),
                "app.environment.removed",
                Some(serde_json::json!({ "slug": slug })),
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Err(deku_core::error::DekuError::EnvironmentNotFound(_)) => {
            not_found(format!("environment '{slug}' not found")).into_response()
        }
        Err(deku_core::error::DekuError::InvalidInput(message)) => {
            bad_request(message).into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct AlertsQuery {
    /// Include resolved alerts, newest first, instead of only active ones.
    #[serde(default)]
    include_resolved: bool,
}

#[utoipa::path(
    get,
    path = "/api/alerts",
    tag = "system",
    params(("include_resolved" = Option<bool>, Query, description = "Include resolved alerts")),
    responses((status = 200, description = "Active alerts, or alert history when include_resolved is set"))
)]
async fn list_alerts_handler(
    State(state): State<SharedState>,
    Query(params): Query<AlertsQuery>,
) -> impl IntoResponse {
    let result = if params.include_resolved {
        queries::list_alerts(&state.pool, 200).await
    } else {
        queries::list_active_alerts(&state.pool).await
    };

    match result {
        Ok(alerts) => (StatusCode::OK, Json(serde_json::json!(alerts))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/metrics",
    tag = "system",
    responses((status = 200, description = "Prometheus metrics in the text exposition format"))
)]
async fn metrics_handler(State(state): State<SharedState>) -> impl IntoResponse {
    match crate::metrics::snapshot(&state.pool).await {
        Ok(snapshot) => (
            StatusCode::OK,
            [(
                axum::http::header::CONTENT_TYPE,
                "text/plain; version=0.0.4; charset=utf-8",
            )],
            crate::metrics::render(&snapshot),
        )
            .into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

/// Report whether values are encrypted at rest, and how many config values are.
async fn encryption_at_rest_check(state: &SharedState) -> serde_json::Value {
    let (total, encrypted) = queries::count_config_var_encryption(&state.pool)
        .await
        .unwrap_or((0, 0));

    match state.config.at_rest_cipher() {
        Ok(Some(_)) => serde_json::json!({
            "name": "encryption_at_rest",
            "status": "ok",
            "detail": format!("key configured; {encrypted} of {total} config values encrypted, backups encrypted"),
        }),
        // Ciphertext with no key is a failure, not a posture warning: those values
        // cannot be read, so the affected apps cannot deploy.
        Ok(None) if encrypted > 0 => serde_json::json!({
            "name": "encryption_at_rest",
            "status": "fail",
            "detail": format!("no key configured but {encrypted} config value(s) are encrypted and cannot be read; restore the key (DEKU_ENCRYPTION_KEY or [encryption])"),
        }),
        Ok(None) => serde_json::json!({
            "name": "encryption_at_rest",
            "status": "warn",
            "detail": format!("no key configured; {total} config value(s) and backups are stored in the clear (set DEKU_ENCRYPTION_KEY or [encryption])"),
        }),
        Err(error) => serde_json::json!({
            "name": "encryption_at_rest",
            "status": "fail",
            "detail": error.to_string(),
        }),
    }
}

/// Inspect every TLS-enabled app's certificate so expiry is visible before it
/// takes an app down. Returns `None` when no app uses TLS.
async fn tls_certificate_check(pool: &SqlitePool) -> Option<serde_json::Value> {
    use crate::services::letsencrypt::{status, CertLifecycle};

    let apps = queries::list_apps(pool).await.ok()?;
    let tls_apps: Vec<_> = apps.into_iter().filter(|app| app.tls_enabled).collect();
    if tls_apps.is_empty() {
        return None;
    }

    let mut failures: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    for app in &tls_apps {
        match status(pool, &app.name).await {
            Ok(cert) => match cert.lifecycle {
                CertLifecycle::Ok => {}
                CertLifecycle::Expiring => warnings.push(format!(
                    "{}: expires in {} day(s)",
                    app.name,
                    cert.days_remaining.unwrap_or_default()
                )),
                CertLifecycle::Expired => failures.push(format!(
                    "{}: expired {} day(s) ago",
                    app.name,
                    cert.days_remaining.unwrap_or_default().abs()
                )),
                CertLifecycle::Missing => {
                    failures.push(format!("{}: certificate or key file missing", app.name))
                }
                CertLifecycle::Unknown => {
                    failures.push(format!("{}: certificate could not be inspected", app.name))
                }
            },
            Err(error) => failures.push(format!("{}: {error}", app.name)),
        }
    }

    let (state, detail) = if !failures.is_empty() {
        ("fail", failures.join("; "))
    } else if !warnings.is_empty() {
        (
            "warn",
            format!(
                "renew within {} days: {}",
                crate::services::letsencrypt::CERT_EXPIRY_WARNING_DAYS,
                warnings.join("; ")
            ),
        )
    } else {
        ("ok", format!("{} TLS certificate(s) valid", tls_apps.len()))
    };

    Some(serde_json::json!({
        "name": "tls_certificates",
        "status": state,
        "detail": detail,
    }))
}

/// Check that the app configs on disk are the ones Angie actually loads.
///
/// The directory check only proves the files exist. A missing `include` leaves
/// every vhost unread while the directory looks healthy, so the effective
/// configuration is inspected instead of assumed.
async fn angie_includes_check(conf_dir: &std::path::Path) -> serde_json::Value {
    let expected = crate::proxy::app_config_files(conf_dir);
    if expected.is_empty() {
        return serde_json::json!({
            "name": "angie_includes",
            "status": "ok",
            "detail": "no app configs to load yet",
        });
    }

    let dump = match crate::proxy::dump().await {
        Ok(dump) => dump,
        Err(error) => {
            // Without a dump there is nothing to conclude, which is a warning
            // rather than a failure.
            return serde_json::json!({
                "name": "angie_includes",
                "status": "warn",
                "detail": format!("could not read the effective Angie config: {error}"),
            });
        }
    };

    let missing =
        crate::proxy::missing_app_configs(&expected, &crate::proxy::loaded_config_files(&dump));

    if missing.is_empty() {
        serde_json::json!({
            "name": "angie_includes",
            "status": "ok",
            "detail": format!("{} app config(s) loaded", expected.len()),
        })
    } else {
        serde_json::json!({
            "name": "angie_includes",
            "status": "fail",
            "detail": format!(
                "{} of {} app config(s) are not loaded by Angie ({:?}); the vhosts exist but are not served, so an include for {} is missing",
                missing.len(),
                expected.len(),
                missing,
                conf_dir.display()
            ),
        })
    }
}

#[utoipa::path(
    get,
    path = "/api/doctor",
    tag = "system",
    responses((status = 200, description = "Host and daemon checks", body = [openapi::DoctorCheckSchema]))
)]
async fn doctor(State(state): State<SharedState>) -> impl IntoResponse {
    let mut checks: Vec<serde_json::Value> = Vec::new();

    checks.push(serde_json::json!({
        "name": "daemon",
        "status": "ok",
        "detail": format!("dekud {}", deku_core::version::release_version()),
    }));

    match sqlx::query("SELECT 1").execute(&state.pool).await {
        Ok(_) => checks.push(serde_json::json!({ "name": "database", "status": "ok", "detail": "query succeeded" })),
        Err(error) => checks.push(serde_json::json!({ "name": "database", "status": "fail", "detail": error.to_string() })),
    }

    match state.docker.ping().await {
        Ok(_) => checks.push(
            serde_json::json!({ "name": "docker", "status": "ok", "detail": "daemon reachable" }),
        ),
        Err(error) => checks.push(
            serde_json::json!({ "name": "docker", "status": "fail", "detail": error.to_string() }),
        ),
    }

    let angie_dir = &state.config.angie_conf_dir;
    if angie_dir.is_dir() {
        checks.push(serde_json::json!({ "name": "angie_config_dir", "status": "ok", "detail": angie_dir.display().to_string() }));
    } else {
        checks.push(serde_json::json!({ "name": "angie_config_dir", "status": "warn", "detail": format!("{} is not a directory", angie_dir.display()) }));
    }
    checks.push(angie_includes_check(angie_dir).await);

    if state.config.object_store.is_some() {
        checks.push(
            serde_json::json!({ "name": "object_store", "status": "ok", "detail": "configured" }),
        );
    } else {
        checks.push(serde_json::json!({ "name": "object_store", "status": "warn", "detail": "not configured; backups unavailable" }));
    }

    let apps = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM apps")
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0);
    let services = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM services")
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0);
    checks.push(serde_json::json!({
        "name": "inventory",
        "status": "ok",
        "detail": format!("{apps} apps, {services} services"),
    }));

    if let Some(check) = tls_certificate_check(&state.pool).await {
        checks.push(check);
    }

    checks.push(encryption_at_rest_check(&state).await);

    let overall = if checks.iter().any(|check| check["status"] == "fail") {
        "degraded"
    } else {
        "ok"
    };
    (
        StatusCode::OK,
        Json(serde_json::json!({ "status": overall, "checks": checks })),
    )
        .into_response()
}

// ── Events ────────────────────────────────────────────────────────────────────
#[derive(Debug, Deserialize)]
struct EventQuery {
    app: Option<String>,
    since: Option<DateTime<Utc>>,
}

#[utoipa::path(
    get,
    path = "/api/events",
    tag = "events",
    params(("app" = Option<String>, Query, description = "Filter by app name"), ("since" = Option<String>, Query, description = "Only events after this RFC 3339 timestamp")),
    responses((status = 200, description = "List recent events"))
)]
async fn list_events(
    State(state): State<SharedState>,
    Query(params): Query<EventQuery>,
) -> impl IntoResponse {
    match queries::list_events(&state.pool, params.app.as_deref(), params.since).await {
        Ok(events) => (StatusCode::OK, Json(serde_json::json!(events))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/events/stream",
    tag = "events",
    responses((status = 200, description = "Stream all events as SSE"))
)]
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

#[utoipa::path(
    get,
    path = "/api/apps/{name}/events/stream",
    tag = "events",
    params(("name" = String, Path, description = "App name"), ("since" = Option<String>, Query, description = "Replay events after this RFC 3339 timestamp")),
    responses((status = 200, description = "Stream events for an app as SSE"))
)]
async fn stream_app_events(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Query(params): Query<StreamQuery>,
) -> Response {
    let _ = params.since;
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(app) => app,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(other) => return internal_error(other).into_response(),
    };

    let app_id = app.id;

    let history = if params.since.is_some() {
        let since = params
            .since
            .as_deref()
            .and_then(|value| value.parse::<DateTime<Utc>>().ok());
        queries::list_events(&state.pool, Some(&app_id), since)
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let history_stream = futures::stream::iter(history.into_iter().rev().filter_map(|event| {
        serde_json::to_string(&event)
            .ok()
            .map(|data| Ok::<SseEvent, Infallible>(SseEvent::default().data(data)))
    }));

    let rx = state.events.subscribe();
    let live = BroadcastStream::new(rx).filter_map(move |result| {
        let app_id = app_id.clone();
        match result {
            Ok(event) if event.app_id.as_ref() == Some(&app_id) => serde_json::to_string(&event)
                .ok()
                .map(|data| Ok(SseEvent::default().data(data))),
            _ => None,
        }
    });

    Sse::new(history_stream.chain(live))
        .keep_alive(KeepAlive::default())
        .into_response()
}

// ── Domains ───────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct AddDomainBody {
    domain: String,
}

#[utoipa::path(
    get,
    path = "/api/apps/{name}/domains",
    tag = "domains",
    params(("name" = String, Path, description = "App name")),
    responses((status = 200, description = "List app domains"))
)]
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

#[utoipa::path(
    post,
    path = "/api/apps/{name}/domains",
    tag = "domains",
    params(("name" = String, Path, description = "App name")),
    request_body = AddDomainBody,
    responses((status = 201, description = "Add a domain to an app"))
)]
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

#[utoipa::path(
    delete,
    path = "/api/apps/{name}/domains/{domain}",
    tag = "domains",
    params(("name" = String, Path, description = "App name"), ("domain" = String, Path, description = "Domain")),
    responses((status = 204, description = "No content"))
)]
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

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct AddPortBody {
    host_port: i64,
    container_port: i64,
    #[serde(default = "default_protocol")]
    protocol: String,
}

fn default_protocol() -> String {
    "tcp".to_string()
}

#[utoipa::path(
    get,
    path = "/api/apps/{name}/ports",
    tag = "ports",
    params(("name" = String, Path, description = "App name")),
    responses((status = 200, description = "List published ports"))
)]
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

#[utoipa::path(
    post,
    path = "/api/apps/{name}/ports",
    tag = "ports",
    params(("name" = String, Path, description = "App name")),
    request_body = AddPortBody,
    responses((status = 201, description = "Publish a port"))
)]
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

#[utoipa::path(
    delete,
    path = "/api/apps/{name}/ports/{id}",
    tag = "ports",
    params(("name" = String, Path, description = "App name"), ("id" = String, Path, description = "Port id")),
    responses((status = 204, description = "No content"))
)]
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

#[utoipa::path(
    get,
    path = "/api/routing",
    tag = "routing",
    responses((status = 200, description = "List the routing table"))
)]
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
    hostnames: Vec<crate::proxy::DerivedHostname>,
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

#[utoipa::path(
    get,
    path = "/api/routing/status",
    tag = "routing",
    responses((status = 200, description = "Routing status summary"))
)]
async fn get_routing_status(State(state): State<SharedState>) -> impl IntoResponse {
    match build_routing_status_response(&state).await {
        Ok(status) => (StatusCode::OK, Json(serde_json::json!(status))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/routing/status/{name}",
    tag = "routing",
    params(("name" = String, Path, description = "App name")),
    responses((status = 200, description = "Routing status for one app"))
)]
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
#[utoipa::path(
    post,
    path = "/api/routing/{name}",
    tag = "routing",
    params(("name" = String, Path, description = "App name")),
    request_body = openapi::RoutingBodySchema,
    responses((status = 204, description = "No content"))
)]
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

    let extras = match crate::proxy::load_extras(&state.pool, &app.id).await {
        Ok(extras) => extras,
        Err(e) => return internal_error(e).into_response(),
    };
    // Manual upstream overrides describe the app's production vhost.
    let environment = match queries::ensure_production_environment(&state.pool, &app.id).await {
        Ok(environment) => environment,
        Err(e) => return internal_error(e).into_response(),
    };
    let desired = Some(crate::proxy::DesiredAppConfig {
        environment_id: &environment.id,
        domains: &domains,
        upstreams: &body.upstreams,
        tls: app.tls_enabled,
        auth: extras.auth.as_ref(),
        maintenance: extras.maintenance,
        maintenance_message: extras.maintenance_message.as_deref(),
        redirects: &extras.redirects,
    });

    if let Err(e) = crate::proxy::apply_app_config(
        &state.pool,
        &state.config.angie_conf_dir,
        state.config.global_domain.as_deref(),
        &app.id,
        &name,
        desired,
    )
    .await
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
    crate::proxy::reconcile_app(
        &state.pool,
        &state.config.angie_conf_dir,
        state.config.global_domain.as_deref(),
        app_id,
        app_name,
    )
    .await
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

#[utoipa::path(
    get,
    path = "/api/apps/{name}/deployments",
    tag = "deploy",
    params(("name" = String, Path, description = "App name")),
    responses((status = 200, description = "List deployments for an app"))
)]
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
    let deps = match queries::list_deployments(&state.pool, &app.id).await {
        Ok(deps) => deps,
        Err(e) => return internal_error(e).into_response(),
    };

    // A deployment is reachable at its own URL only once a global domain exists
    // and while its containers are retained. The URL is derived rather than
    // stored, so it is computed here — but only for deployments that still have
    // running web containers, since a retired one's vhost has been removed.
    let slugs: std::collections::HashMap<String, String> =
        queries::list_deployment_environment_slugs(&state.pool, &app.id)
            .await
            .unwrap_or_default()
            .into_iter()
            .collect();

    let mut reachable: std::collections::HashSet<String> = std::collections::HashSet::new();
    if state.config.global_domain.is_some() {
        for environment in queries::list_environments(&state.pool, &app.id)
            .await
            .unwrap_or_default()
        {
            for (deployment_id, ports) in
                queries::list_deployment_upstream_ports(&state.pool, &app.id, &environment.id)
                    .await
                    .unwrap_or_default()
            {
                if !ports.is_empty() {
                    reachable.insert(deployment_id);
                }
            }
        }
    }

    let deployments: Vec<serde_json::Value> = deps
        .iter()
        .map(|deployment| {
            let mut value = serde_json::to_value(deployment).unwrap_or_default();
            let url = slugs
                .get(&deployment.id)
                .filter(|_| reachable.contains(&deployment.id))
                .and_then(|slug| {
                    crate::proxy::deployment_hostname(
                        &app.name,
                        slug,
                        &deployment.id,
                        state.config.global_domain.as_deref(),
                    )
                })
                .map(|hostname| format!("http://{hostname}"));
            if let Some(url) = url {
                value["preview_url"] = serde_json::Value::String(url);
            }
            value
        })
        .collect();

    (StatusCode::OK, Json(serde_json::json!(deployments))).into_response()
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct DeployBody {
    /// "source" | "image" | "archive"
    source: String,
    /// Image reference (when source = "image")
    image: Option<String>,
    /// Archive path or URL (when source = "archive")
    archive: Option<String>,
    /// Force a specific builder
    builder: Option<String>,
    /// Build host selection: omit for the configured default, "local" to build
    /// on the deploy host, or the configured build host name.
    build_host: Option<String>,
    /// Environment slug to deploy into. Omit for production.
    environment: Option<String>,
}

/// Resolve a deploy's target environment id, or a response describing why not.
///
/// Resolving here, before the deploy is spawned, is what lets an unknown
/// environment fail the request instead of the background job: `deploy run
/// --environment typo` otherwise returns 202 and only fails once it is already
/// streaming.
async fn resolve_deploy_environment(
    state: &AppState,
    app_id: &str,
    slug: Option<&str>,
) -> std::result::Result<String, (StatusCode, Json<serde_json::Value>)> {
    match slug {
        Some(slug) => match queries::get_environment(&state.pool, app_id, slug).await {
            Ok(environment) => Ok(environment.id),
            Err(_) => {
                let available = queries::list_environments(&state.pool, app_id)
                    .await
                    .map(|environments| {
                        environments
                            .into_iter()
                            .map(|environment| environment.slug)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                Err(bad_request(format!(
                    "environment '{slug}' not found; available: {}",
                    available.join(", ")
                )))
            }
        },
        None => match queries::ensure_production_environment(&state.pool, app_id).await {
            Ok(environment) => Ok(environment.id),
            Err(e) => Err(internal_error(e)),
        },
    }
}

#[utoipa::path(
    post,
    path = "/api/apps/{name}/deploy",
    tag = "deploy",
    params(("name" = String, Path, description = "App name")),
    request_body = DeployBody,
    responses((status = 202, description = "Deploy an app"))
)]
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

    let environment_id =
        match resolve_deploy_environment(&state, &app.id, body.environment.as_deref()).await {
            Ok(id) => id,
            Err(response) => return response.into_response(),
        };

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
        build_host: body.build_host,
        environment_id: Some(environment_id),
    };

    let pool = state.pool.clone();
    let docker = state.docker.clone();
    let events = state.events.clone();
    let logs = state.logs.clone();
    let cfg = current_config(&state);
    let plugins = state.plugins.clone();
    let app_id = app.id.clone();
    let deploy_lock = state.deploy_locks.for_app(&app.id);

    // Run deploy in background so the HTTP response returns immediately
    tokio::spawn(async move {
        let _guard = deploy_lock.lock().await;
        if let Err(e) =
            crate::deploy::run_deploy(&pool, &docker, &events, &logs, &cfg, plugins.as_ref(), req)
                .await
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

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct RollbackBody {
    deployment_id: Option<String>,
    /// Environment slug to roll back within. Omit for production.
    environment: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/apps/{name}/rollback",
    tag = "deploy",
    params(("name" = String, Path, description = "App name")),
    request_body = RollbackBody,
    responses((status = 202, description = "Roll back to a previous deployment"))
)]
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
    let logs = state.logs.clone();
    let cfg = state.config.clone();
    let plugins = state.plugins.clone();
    // Resolve before spawning so an unknown environment fails the request.
    let environment_id =
        match resolve_deploy_environment(&state, &app.id, body.environment.as_deref()).await {
            Ok(id) => id,
            Err(response) => return response.into_response(),
        };

    // An explicit target must belong to the environment being rolled back.
    // Checked here so the caller hears about it, instead of the spawned job
    // rejecting it after the response has already said the rollback started.
    if let Some(target_id) = body.deployment_id.as_deref() {
        match queries::get_deployment(&state.pool, target_id).await {
            Ok(target) => {
                let target_environment =
                    queries::get_deployment_environment_id(&state.pool, &target.id)
                        .await
                        .unwrap_or(None);
                if target_environment.as_deref() != Some(environment_id.as_str()) {
                    return bad_request(format!(
                        "deployment {target_id} does not belong to that environment"
                    ))
                    .into_response();
                }
            }
            Err(_) => {
                return not_found(format!("deployment '{target_id}' not found")).into_response();
            }
        }
    }

    let app_id = app.id.clone();
    let to_id = body.deployment_id.clone();
    let deploy_lock = state.deploy_locks.for_app(&app.id);

    tokio::spawn(async move {
        let _guard = deploy_lock.lock().await;
        if let Err(e) = crate::deploy::rollback(
            &pool,
            &docker,
            &events,
            &logs,
            &cfg,
            plugins.as_ref(),
            &app_id,
            &name,
            Some(&environment_id),
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

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct LogsQuery {
    /// Number of lines to return.
    #[serde(default = "default_log_lines")]
    n: usize,
    /// Case-insensitive full-text search over stored lines.
    search: Option<String>,
    /// Only lines from this deployment.
    deployment: Option<String>,
    /// Only lines from this environment.
    environment: Option<String>,
    /// `build` or `runtime`.
    source: Option<String>,
    /// `stdout` or `stderr`.
    stream: Option<String>,
    /// Inferred level, for example `ERROR`.
    level: Option<String>,
}

impl LogsQuery {
    fn filter(&self) -> crate::logs::LogQuery {
        crate::logs::LogQuery {
            search: self.search.clone(),
            deployment: self.deployment.clone(),
            environment: self.environment.clone(),
            source: self.source.clone(),
            stream: self.stream.clone(),
            level: self.level.clone(),
            limit: Some(self.n as i64),
        }
    }
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

#[derive(Debug, Clone, Serialize)]
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
    /// Result for the first running web replica, kept for existing consumers.
    probe: Option<AppChecksProbeResult>,
    /// One result per running web replica; a failure here is a failure even when
    /// `probe` succeeded.
    probes: Vec<AppChecksProbeResult>,
    issues: Vec<String>,
}

#[utoipa::path(
    get,
    path = "/api/apps/{name}/logs",
    tag = "logs",
    params(("name" = String, Path, description = "App name"), ("n" = usize, Query, description = "Number of log lines"), ("search" = Option<String>, Query, description = "Full-text search over stored lines"), ("deployment" = Option<String>, Query, description = "Filter by deployment id"), ("environment" = Option<String>, Query, description = "Filter by environment slug"), ("source" = Option<String>, Query, description = "build or runtime"), ("stream" = Option<String>, Query, description = "stdout or stderr"), ("level" = Option<String>, Query, description = "Inferred log level")),
    responses((status = 200, description = "Stored log lines for an app, most recent last"))
)]
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

    // Stored lines cover build output and runtime output for every deployment,
    // including deployments whose containers have since been retired.
    let lines = match crate::logs::query(&state.pool, &app.id, &params.filter()).await {
        Ok(lines) => lines,
        Err(e) => return internal_error(e).into_response(),
    };

    // `logs` stays a plain string array so existing callers keep working; `lines`
    // carries the structured records.
    let plain: Vec<&str> = lines
        .iter()
        .map(|record| record.line.message.as_str())
        .collect();
    (
        StatusCode::OK,
        Json(serde_json::json!({ "logs": plain, "lines": lines })),
    )
        .into_response()
}

#[utoipa::path(
    get,
    path = "/api/apps/{name}/logs/stream",
    tag = "logs",
    params(("name" = String, Path, description = "App name"), ("source" = Option<String>, Query, description = "build or runtime"), ("stream" = Option<String>, Query, description = "stdout or stderr"), ("level" = Option<String>, Query, description = "Inferred log level")),
    responses((status = 200, description = "Stream stored log lines as SSE"))
)]
async fn stream_app_logs(
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

    let filter = params.filter();
    let app_id = app.id.clone();
    let receiver = state.logs.subscribe();

    // A plain closure: every filter is in memory, so there is nothing to await.
    let stream = BroadcastStream::new(receiver).filter_map(move |result| {
        let line = result.ok()?;
        if line.app_id != app_id {
            return None;
        }

        let wants = |wanted: &Option<String>, actual: &str| match wanted.as_deref() {
            Some(wanted) if !wanted.trim().is_empty() => wanted.trim() == actual,
            _ => true,
        };
        if !wants(&filter.source, &line.source)
            || !wants(&filter.stream, &line.stream)
            || !wants(&filter.level, &line.level)
        {
            return None;
        }

        serde_json::to_string(&line)
            .ok()
            .map(|data| Ok::<SseEvent, std::convert::Infallible>(SseEvent::default().data(data)))
    });

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

#[utoipa::path(
    get,
    path = "/api/apps/{name}/checks",
    tag = "apps",
    params(("name" = String, Path, description = "App name"), ("path" = Option<String>, Query, description = "Health check path"), ("timeout_secs" = Option<u64>, Query, description = "Health check timeout in seconds")),
    responses((status = 200, description = "Run health checks for an app"))
)]
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

    // Probe every running web replica. Checking only one would report a healthy
    // app while another replica is serving errors.
    let check_environment = match queries::ensure_production_environment(&state.pool, &app.id).await
    {
        Ok(environment) => environment,
        Err(e) => return internal_error(e).into_response(),
    };
    let upstream_ports =
        match queries::list_web_upstream_ports(&state.pool, &app.id, &check_environment.id).await {
            Ok(ports) => ports,
            Err(e) => return internal_error(e).into_response(),
        };

    let path = normalize_check_path(params.path.as_deref().unwrap_or("/"));
    let timeout_secs = params.timeout_secs.unwrap_or(5);
    let mut probes = Vec::with_capacity(upstream_ports.len());
    for port in &upstream_ports {
        probes.push(run_live_http_probe(*port, &path, timeout_secs).await);
    }
    let probe = probes.first().cloned();

    let mut issues = Vec::new();
    if latest_deployment.is_none() {
        issues.push("no live deployment recorded".to_string());
    }
    if containers.is_empty() {
        issues.push("no running containers recorded".to_string());
    }
    if probes.is_empty() {
        issues.push("no running web replica available for an HTTP probe".to_string());
    }
    for (replica, result) in probes.iter().enumerate() {
        if !result.ok {
            let reason = result
                .error
                .clone()
                .unwrap_or_else(|| match result.status_code {
                    Some(status) => format!("HTTP {status}"),
                    None => "no response".to_string(),
                });
            issues.push(format!(
                "local HTTP probe failed for web replica {replica} at {}{}: {reason}",
                result.target, result.path
            ));
        }
    }
    issues.extend(routing.issues.iter().cloned());

    let has_fatal_issue = latest_deployment.is_none()
        || containers.is_empty()
        || probes.iter().any(|probe| !probe.ok);

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
            probes,
            issues,
        })),
    )
        .into_response()
}

// ── Config vars ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ConfigQuery {
    environment: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/apps/{name}/config",
    tag = "config",
    params(("name" = String, Path, description = "App name"), ("environment" = Option<String>, Query, description = "Environment slug; returns that environment's effective set")),
    responses((status = 200, description = "List config vars"))
)]
async fn list_config(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Query(params): Query<ConfigQuery>,
) -> impl IntoResponse {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(a) => a,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };
    let listing = match params.environment.as_deref() {
        Some(slug) => match queries::get_environment(&state.pool, &app.id, slug).await {
            Ok(environment) => {
                crate::secrets::list_config_vars_for_environment(
                    &state.pool,
                    &state.config,
                    &app.id,
                    &environment.id,
                )
                .await
            }
            Err(e) => return internal_error(e).into_response(),
        },
        None => crate::secrets::list_config_vars(&state.pool, &state.config, &app.id).await,
    };

    match listing {
        Ok(vars) => (StatusCode::OK, Json(serde_json::json!(vars))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct SetConfigBody {
    key: String,
    value: String,
    #[serde(default)]
    is_global: bool,
    /// Environment slug to override in. Omit for the app-wide value.
    environment: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/apps/{name}/config",
    tag = "config",
    params(("name" = String, Path, description = "App name")),
    request_body = SetConfigBody,
    responses((status = 204, description = "No content"))
)]
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
    // An override is written for one environment; without a slug the value is
    // app-wide and every environment inherits it.
    let write = match body.environment.as_deref() {
        Some(slug) => match queries::get_environment(&state.pool, &app.id, slug).await {
            Ok(environment) => {
                crate::secrets::set_environment_config_var(
                    &state.pool,
                    &state.config,
                    &app.id,
                    &environment.id,
                    &body.key,
                    &body.value,
                    body.is_global,
                )
                .await
            }
            Err(e) => return internal_error(e).into_response(),
        },
        None => {
            crate::secrets::set_config_var(
                &state.pool,
                &state.config,
                &app.id,
                &body.key,
                &body.value,
                body.is_global,
            )
            .await
        }
    };

    match write {
        Ok(()) => {
            state.events.emit(
                Some(app.id),
                "config.set",
                Some(serde_json::json!({
                    "key": body.key,
                    "environment": body.environment,
                })),
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/api/apps/{name}/config/{key}",
    tag = "config",
    params(("name" = String, Path, description = "App name"), ("key" = String, Path, description = "Config key"), ("environment" = Option<String>, Query, description = "Environment slug; removes that environment's override only")),
    responses((status = 204, description = "No content"))
)]
async fn unset_config(
    State(state): State<SharedState>,
    axum::extract::Path((name, key)): axum::extract::Path<(String, String)>,
    Query(params): Query<ConfigQuery>,
) -> impl IntoResponse {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(a) => a,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };
    let removal = match params.environment.as_deref() {
        Some(slug) => match queries::get_environment(&state.pool, &app.id, slug).await {
            Ok(environment) => {
                queries::unset_environment_config_var(&state.pool, &app.id, &environment.id, &key)
                    .await
            }
            Err(e) => return internal_error(e).into_response(),
        },
        None => queries::unset_config_var(&state.pool, &app.id, &key).await,
    };

    match removal {
        Ok(()) => {
            state.events.emit(
                Some(app.id),
                "config.unset",
                Some(serde_json::json!({
                    "key": key,
                    "environment": params.environment,
                })),
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

// ── Process inspection / scale ────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/apps/{name}/ps",
    tag = "ps",
    params(("name" = String, Path, description = "App name")),
    responses((status = 200, description = "List running processes for an app"))
)]
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

#[utoipa::path(
    get,
    path = "/api/apps/{name}/scale",
    tag = "ps",
    params(("name" = String, Path, description = "App name")),
    responses((status = 200, description = "Show process scale"))
)]
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

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct SetLimitsBody {
    /// Process type to scope the limit to; defaults to every process.
    process_type: Option<String>,
    /// CPU limit: cores (`0.5`) or millicores (`500m`).
    cpu: Option<String>,
    /// Memory limit: `512m`, `1g`, `1024k`, or bytes.
    memory: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/apps/{name}/limits",
    tag = "limits",
    params(("name" = String, Path, description = "App name")),
    responses((status = 200, description = "Resource limits", body = [openapi::LimitsSchema]))
)]
async fn get_limits(
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
    match queries::list_resource_limits(&state.pool, &app.id).await {
        Ok(limits) => (StatusCode::OK, Json(serde_json::json!(limits))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/apps/{name}/limits",
    tag = "limits",
    params(("name" = String, Path, description = "App name")),
request_body = openapi::LimitsSchema,
    responses((status = 200, description = "Limits set"), (status = 400, description = "Invalid limit"))
)]
async fn set_limits(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(body): Json<SetLimitsBody>,
) -> impl IntoResponse {
    let app = match queries::get_app(&state.pool, &name).await {
        Ok(app) => app,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return not_found(format!("app '{name}' not found")).into_response();
        }
        Err(e) => return internal_error(e).into_response(),
    };

    if body.cpu.is_none() && body.memory.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "provide at least one of cpu or memory" })),
        )
            .into_response();
    }
    if let Some(cpu) = body.cpu.as_deref() {
        if crate::deploy::parse_cpu_quota(cpu).is_none() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("invalid cpu limit '{cpu}'") })),
            )
                .into_response();
        }
    }
    if let Some(memory) = body.memory.as_deref() {
        if crate::deploy::parse_memory_bytes(memory).is_none() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("invalid memory limit '{memory}'") })),
            )
                .into_response();
        }
    }

    let process_type = body.process_type.as_deref().unwrap_or("_all_");
    match queries::set_resource_limit(
        &state.pool,
        &app.id,
        process_type,
        body.cpu.as_deref(),
        body.memory.as_deref(),
    )
    .await
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "process_type": process_type,
                "cpu": body.cpu,
                "memory": body.memory,
                "note": "applies on the next deploy",
            })),
        )
            .into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct SetScaleBody {
    /// Map of process_type → count
    scales: std::collections::HashMap<String, i64>,
}

#[utoipa::path(
    post,
    path = "/api/apps/{name}/scale",
    tag = "ps",
    params(("name" = String, Path, description = "App name")),
    request_body = SetScaleBody,
    responses((status = 204, description = "No content"))
)]
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
        let environment = queries::ensure_production_environment(&state.pool, &app.id).await?;
        let upstreams = queries::list_web_upstreams(&state.pool, &app.id, &environment.id).await?;
        // Environment and per-deployment hostnames are served too, and are
        // derived rather than stored, so they are read from the same place the
        // proxy writes them.
        let hostnames = crate::proxy::derived_hostnames(
            &state.pool,
            &app.id,
            &app.name,
            state.config.global_domain.as_deref(),
        )
        .await?;
        table.push(serde_json::json!({
            "app": app.name,
            "domains": domains,
            "hostnames": hostnames,
            "upstreams": upstreams.iter().map(|upstream| serde_json::json!({
                "host": upstream.host,
                "port": upstream.port,
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

/// What an app's proxy config is built from, and what the status says about it.
///
/// Kept apart from the app so the rule can be checked on its own: what counts as
/// complete routing inputs has to be the rule the proxy writes by, or the status
/// reports a problem for an app that is serving.
struct RoutingInputs<'a> {
    domains: &'a [String],
    global_domain: Option<&'a str>,
    upstreams_present: bool,
    proxy_config_present: bool,
}

impl<'a> RoutingInputs<'a> {
    /// Whether there is a host to route on: a domain of the app's own, or a
    /// global domain to derive an environment hostname from.
    fn routable(&self) -> bool {
        crate::proxy::has_routable_hosts(self.domains, self.global_domain)
    }

    /// Whether there is both something to route on and something to route to.
    fn inputs_complete(&self) -> bool {
        self.routable() && self.upstreams_present
    }

    fn issues(&self) -> Vec<String> {
        let mut issues = Vec::new();
        if !self.routable() {
            issues
                .push("no domains configured and no global domain to derive one from".to_string());
        }
        if !self.upstreams_present {
            issues.push("no upstreams configured".to_string());
        }

        if self.inputs_complete() && !self.proxy_config_present {
            issues.push("angie config fragment is missing".to_string());
        }
        if !self.inputs_complete() && self.proxy_config_present {
            issues.push("angie config fragment exists without complete routing inputs".to_string());
        }

        issues
    }
}

async fn build_routing_status_for_app(
    state: &AppState,
    app: &deku_core::types::App,
) -> anyhow::Result<RoutingAppStatus> {
    let domains = queries::list_domain_names(&state.pool, &app.id).await?;
    let environment = queries::ensure_production_environment(&state.pool, &app.id).await?;
    let upstreams = queries::list_web_upstreams(&state.pool, &app.id, &environment.id).await?;
    let hostnames = crate::proxy::derived_hostnames(
        &state.pool,
        &app.id,
        &app.name,
        state.config.global_domain.as_deref(),
    )
    .await?;
    let proxy_config_path = crate::proxy::app_config_path(&state.config.angie_conf_dir, &app.name);
    let proxy_config_present = proxy_config_path.exists();
    let tls_status = crate::services::letsencrypt::status(&state.pool, &app.name).await?;
    let inputs = RoutingInputs {
        domains: &domains,
        global_domain: state.config.global_domain.as_deref(),
        upstreams_present: !upstreams.is_empty(),
        proxy_config_present,
    };
    let mut issues = inputs.issues();
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
    } else if !inputs.inputs_complete() {
        "pending"
    } else {
        "degraded"
    };

    Ok(RoutingAppStatus {
        app: app.name.clone(),
        status: status.to_string(),
        domains,
        hostnames,
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

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct AddSshKeyBody {
    name: String,
    public_key: String,
}

#[utoipa::path(
    get,
    path = "/api/ssh-keys",
    tag = "ssh-keys",
    responses((status = 200, description = "List SSH keys"))
)]
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

#[utoipa::path(
    post,
    path = "/api/ssh-keys",
    tag = "ssh-keys",
    request_body = AddSshKeyBody,
    responses((status = 201, description = "Add an SSH key"))
)]
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

#[utoipa::path(
    delete,
    path = "/api/ssh-keys/{name}",
    tag = "ssh-keys",
    params(("name" = String, Path, description = "Key name")),
    responses((status = 204, description = "No content"))
)]
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

#[utoipa::path(
    get,
    path = "/api/plugins",
    tag = "plugins",
    responses((status = 200, description = "List installed plugins"))
)]
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

#[utoipa::path(
    get,
    path = "/api/plugins/runtime",
    tag = "plugins",
    responses((status = 200, description = "Dynamic plugin runtime availability", body = openapi::PluginRuntimeSchema))
)]
async fn get_plugins_runtime() -> impl IntoResponse {
    let available = crate::plugins::dynamic_runtime_available();
    let mut payload = serde_json::json!({ "available": available });
    if !available {
        payload["message"] =
            serde_json::Value::String(crate::plugins::runtime_unavailable_message().to_string());
    }
    (StatusCode::OK, Json(payload)).into_response()
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct InstallPluginBody {
    path: String,
}

#[utoipa::path(
    post,
    path = "/api/plugins",
    tag = "plugins",
    request_body = InstallPluginBody,
    responses((status = 201, description = "Install a plugin"))
)]
async fn install_plugin(
    State(state): State<SharedState>,
    Json(body): Json<InstallPluginBody>,
) -> impl IntoResponse {
    if !crate::plugins::dynamic_runtime_available() {
        return (
            StatusCode::NOT_IMPLEMENTED,
            Json(serde_json::json!({
                "error": crate::plugins::runtime_unavailable_message(),
            })),
        )
            .into_response();
    }

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

#[utoipa::path(
    delete,
    path = "/api/plugins/{name}",
    tag = "plugins",
    params(("name" = String, Path, description = "Plugin name")),
    responses((status = 204, description = "No content"))
)]
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

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct ImportConfigBody {
    /// Config vars to import, keyed by name.
    vars: std::collections::BTreeMap<String, String>,
    /// Replace vars that already exist instead of skipping them.
    #[serde(default)]
    overwrite: bool,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct RenameAppBody {
    /// New name for the app.
    name: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct CreateDeployTokenBody {
    /// Label for the token, so several can be told apart.
    name: String,
}

fn deploy_token_json(token: &queries::DeployToken) -> serde_json::Value {
    serde_json::json!({
        "id": token.id,
        "name": token.name,
        "prefix": token.token_prefix,
        "created_at": token.created_at,
        "last_used_at": token.last_used_at,
    })
}

#[derive(Debug, Deserialize)]
struct ArchiveDeployQuery {
    builder: Option<String>,
    build_host: Option<String>,
    environment: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/apps/{name}/deploy/archive",
    tag = "deploy",
    params(("name" = String, Path, description = "App name"), ("builder" = Option<String>, Query, description = "Force a specific builder"), ("build_host" = Option<String>, Query, description = "Build host: configured default, 'local', or the configured name"), ("environment" = Option<String>, Query, description = "Environment slug to deploy into; defaults to production")),
    responses((status = 202, description = "Deploy an app from an uploaded source archive"))
)]
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

    let environment_id =
        match resolve_deploy_environment(&state, &app.id, params.environment.as_deref()).await {
            Ok(id) => id,
            Err(response) => return response.into_response(),
        };

    // Stream the archive field straight to a temp file so a permitted body is
    // never held in memory.
    let mut archive_path: Option<std::path::PathBuf> = None;
    loop {
        match multipart.next_field().await {
            Ok(Some(mut field)) if field.name() == Some("archive") => {
                let tmp = match tempfile::NamedTempFile::new() {
                    Ok(tmp) => tmp,
                    Err(e) => return internal_error(e).into_response(),
                };
                let (file, path) = match tmp.keep() {
                    Ok(pair) => pair,
                    Err(e) => return internal_error(e.error).into_response(),
                };
                let mut file = tokio::fs::File::from_std(file);
                let mut total: u64 = 0;
                loop {
                    match field.chunk().await {
                        Ok(Some(chunk)) => {
                            total += chunk.len() as u64;
                            if total > ARCHIVE_UPLOAD_LIMIT as u64 {
                                let _ = std::fs::remove_file(&path);
                                return (
                                    StatusCode::PAYLOAD_TOO_LARGE,
                                    Json(serde_json::json!({
                                        "error": format!(
                                            "archive exceeds the {} MiB upload limit",
                                            ARCHIVE_UPLOAD_LIMIT / (1024 * 1024)
                                        )
                                    })),
                                )
                                    .into_response();
                            }
                            if let Err(e) = file.write_all(&chunk).await {
                                let _ = std::fs::remove_file(&path);
                                return internal_error(e).into_response();
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            let _ = std::fs::remove_file(&path);
                            return multipart_error(e);
                        }
                    }
                }
                if let Err(e) = file.flush().await {
                    let _ = std::fs::remove_file(&path);
                    return internal_error(e).into_response();
                }
                archive_path = Some(path);
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(e) => return multipart_error(e),
        }
    }

    let tmp_path = match archive_path {
        Some(path) => path,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "missing 'archive' field in multipart body" })),
            )
                .into_response();
        }
    };
    let path_str = tmp_path.to_string_lossy().to_string();

    let req = DeployRequest {
        app_id: app.id.clone(),
        app_name: name.clone(),
        source: DeploySource::Archive {
            path: path_str.clone(),
        },
        force_builder: params.builder,
        build_host: params.build_host,
        environment_id: Some(environment_id),
    };

    let pool = state.pool.clone();
    let docker = state.docker.clone();
    let events = state.events.clone();
    let logs = state.logs.clone();
    let cfg = current_config(&state);
    let plugins = state.plugins.clone();
    let app_id = app.id.clone();
    let deploy_lock = state.deploy_locks.for_app(&app.id);

    tokio::spawn(async move {
        let _guard = deploy_lock.lock().await;
        if let Err(e) =
            crate::deploy::run_deploy(&pool, &docker, &events, &logs, &cfg, plugins.as_ref(), req)
                .await
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

#[cfg(test)]
mod routing_inputs_tests {
    use super::RoutingInputs;

    fn issues_for(
        domains: &[&str],
        global_domain: Option<&str>,
        upstreams_present: bool,
        proxy_config_present: bool,
    ) -> Vec<String> {
        let domains: Vec<String> = domains.iter().map(|domain| domain.to_string()).collect();
        RoutingInputs {
            domains: &domains,
            global_domain,
            upstreams_present,
            proxy_config_present,
        }
        .issues()
    }

    fn inputs_for(domains: &[&str], global_domain: Option<&str>, upstreams_present: bool) -> bool {
        let domains: Vec<String> = domains.iter().map(|domain| domain.to_string()).collect();
        RoutingInputs {
            domains: &domains,
            global_domain,
            upstreams_present,
            proxy_config_present: true,
        }
        .inputs_complete()
    }

    #[test]
    fn a_global_domain_completes_a_domain_less_app() {
        // The proxy serves this app at its environment and per-deployment
        // hostnames, so the status must not call its inputs incomplete.
        assert!(inputs_for(&[], Some("apps.test"), true));
        assert!(
            issues_for(&[], Some("apps.test"), true, true).is_empty(),
            "a serving app is not an issue"
        );
    }

    #[test]
    fn an_app_with_nothing_to_route_on_is_incomplete() {
        assert!(!inputs_for(&[], None, true));
        let issues = issues_for(&[], None, true, false);
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("no domains configured")),
            "{issues:?}"
        );
    }

    #[test]
    fn a_fragment_without_complete_inputs_is_reported() {
        let issues = issues_for(&[], None, false, true);
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("exists without complete routing inputs")),
            "{issues:?}"
        );
    }

    #[test]
    fn an_app_with_domains_is_incomplete_without_upstreams() {
        assert!(!inputs_for(&["example.com"], None, false));
        assert!(inputs_for(&["example.com"], None, true));
    }
}
