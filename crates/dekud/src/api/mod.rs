use std::future::IntoFuture;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::net::{TcpListener, UnixListener};
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::config::DekuConfig;
use crate::db::queries;
use crate::events::EventSender;
use deku_core::types::NewApp;

pub mod auth;

#[derive(Clone)]
pub struct AppState {
    pub config: DekuConfig,
    pub pool: SqlitePool,
    #[allow(dead_code)]
    pub events: EventSender,
}

impl AppState {
    pub fn new(config: DekuConfig, pool: SqlitePool, events: EventSender) -> Arc<Self> {
        Arc::new(Self { config, pool, events })
    }
}

pub type SharedState = Arc<AppState>;

pub async fn serve(state: SharedState) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/healthz", get(health_check))
        .route("/api/apps", get(list_apps).post(create_app))
        .route("/api/apps/:name", get(get_app).delete(delete_app))
        .route("/api/events", get(list_events))
        .with_state(state.clone())
        .layer(TraceLayer::new_for_http());

    // TCP listener for dashboard + remote access
    let tcp_addr = format!("0.0.0.0:{}", state.config.api_port);
    let tcp_listener = TcpListener::bind(&tcp_addr).await?;
    info!("API listening on {tcp_addr}");

    // Unix socket listener for local CLI
    let sock_path = &state.config.socket_path;
    if sock_path.exists() {
        std::fs::remove_file(sock_path)?;
    }
    std::fs::create_dir_all(sock_path.parent().unwrap())?;
    let unix_listener = UnixListener::bind(sock_path)?;
    info!("API listening on {}", sock_path.display());

    tokio::try_join!(
        axum::serve(tcp_listener, app.clone()).into_future(),
        axum::serve(unix_listener, app).into_future(),
    )?;

    Ok(())
}

async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok", "service": "dekud" }))
}

async fn list_apps(State(state): State<SharedState>) -> impl IntoResponse {
    match queries::list_apps(&state.pool).await {
        Ok(apps) => (StatusCode::OK, Json(serde_json::json!(apps))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn create_app(
    State(state): State<SharedState>,
    Json(body): Json<NewApp>,
) -> impl IntoResponse {
    match queries::create_app(&state.pool, &body).await {
        Ok(app) => (StatusCode::CREATED, Json(serde_json::json!(app))).into_response(),
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
        Err(deku_core::error::DekuError::AppNotFound(_)) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("app '{name}' not found") })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn delete_app(
    State(state): State<SharedState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> impl IntoResponse {
    match queries::delete_app(&state.pool, &name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(deku_core::error::DekuError::AppNotFound(_)) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("app '{name}' not found") })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn list_events(_state: State<SharedState>) -> impl IntoResponse {
    Json(serde_json::json!({ "events": [] }))
}
