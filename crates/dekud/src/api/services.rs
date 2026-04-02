use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

use super::{internal_error, not_found, SharedState};
use crate::db::queries;
use crate::services::{database, letsencrypt, network, storage};

// ── Shared request/response types ─────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateServiceBody {
    pub name: String,
}

#[derive(Deserialize)]
pub struct LogsQuery {
    pub n: Option<usize>,
}

fn service_json(s: &queries::Service) -> serde_json::Value {
    serde_json::json!({
        "id": s.id,
        "name": s.name,
        "status": s.status,
        "plugin": s.plugin,
    })
}

// ── Postgres ──────────────────────────────────────────────────────────────────

pub async fn pg_list(State(state): State<SharedState>) -> impl IntoResponse {
    match queries::list_services(&state.pool, "postgres").await {
        Ok(services) => {
            let json: Vec<_> = services.iter().map(service_json).collect();
            (StatusCode::OK, Json(serde_json::json!(json))).into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn pg_create(
    State(state): State<SharedState>,
    Json(body): Json<CreateServiceBody>,
) -> impl IntoResponse {
    match database::create(&state.pool, &state.docker, database::postgres_spec(), &body.name).await
    {
        Ok(svc) => (StatusCode::CREATED, Json(service_json(&svc))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn pg_info(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match queries::get_service(&state.pool, &name).await {
        Ok(svc) => (StatusCode::OK, Json(service_json(&svc))).into_response(),
        Err(_) => not_found(format!("postgres service '{name}' not found")).into_response(),
    }
}

pub async fn pg_destroy(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match database::destroy(&state.pool, &state.docker, &name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn pg_link(
    State(state): State<SharedState>,
    Path((name, app)): Path<(String, String)>,
) -> impl IntoResponse {
    match database::link(&state.pool, &name, &app, &database::postgres_spec()).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn pg_unlink(
    State(state): State<SharedState>,
    Path((name, app)): Path<(String, String)>,
) -> impl IntoResponse {
    match database::unlink(&state.pool, &name, &app).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn pg_logs(
    State(state): State<SharedState>,
    Path(name): Path<String>,
    Query(q): Query<LogsQuery>,
) -> impl IntoResponse {
    match database::get_logs(&state.docker, "postgres", &name, q.n.unwrap_or(100)).await {
        Ok(logs) => (StatusCode::OK, Json(serde_json::json!({ "logs": logs }))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

// ── Redis ─────────────────────────────────────────────────────────────────────

pub async fn rd_list(State(state): State<SharedState>) -> impl IntoResponse {
    match queries::list_services(&state.pool, "redis").await {
        Ok(services) => {
            let json: Vec<_> = services.iter().map(service_json).collect();
            (StatusCode::OK, Json(serde_json::json!(json))).into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn rd_create(
    State(state): State<SharedState>,
    Json(body): Json<CreateServiceBody>,
) -> impl IntoResponse {
    match database::create(&state.pool, &state.docker, database::redis_spec(), &body.name).await {
        Ok(svc) => (StatusCode::CREATED, Json(service_json(&svc))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn rd_info(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match queries::get_service(&state.pool, &name).await {
        Ok(svc) => (StatusCode::OK, Json(service_json(&svc))).into_response(),
        Err(_) => not_found(format!("redis service '{name}' not found")).into_response(),
    }
}

pub async fn rd_destroy(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match database::destroy(&state.pool, &state.docker, &name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn rd_link(
    State(state): State<SharedState>,
    Path((name, app)): Path<(String, String)>,
) -> impl IntoResponse {
    match database::link(&state.pool, &name, &app, &database::redis_spec()).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn rd_unlink(
    State(state): State<SharedState>,
    Path((name, app)): Path<(String, String)>,
) -> impl IntoResponse {
    match database::unlink(&state.pool, &name, &app).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn rd_logs(
    State(state): State<SharedState>,
    Path(name): Path<String>,
    Query(q): Query<LogsQuery>,
) -> impl IntoResponse {
    match database::get_logs(&state.docker, "redis", &name, q.n.unwrap_or(100)).await {
        Ok(logs) => (StatusCode::OK, Json(serde_json::json!({ "logs": logs }))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

// ── MySQL ─────────────────────────────────────────────────────────────────────

pub async fn my_list(State(state): State<SharedState>) -> impl IntoResponse {
    match queries::list_services(&state.pool, "mysql").await {
        Ok(services) => {
            let json: Vec<_> = services.iter().map(service_json).collect();
            (StatusCode::OK, Json(serde_json::json!(json))).into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn my_create(
    State(state): State<SharedState>,
    Json(body): Json<CreateServiceBody>,
) -> impl IntoResponse {
    match database::create(&state.pool, &state.docker, database::mysql_spec(), &body.name).await {
        Ok(svc) => (StatusCode::CREATED, Json(service_json(&svc))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn my_info(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match queries::get_service(&state.pool, &name).await {
        Ok(svc) => (StatusCode::OK, Json(service_json(&svc))).into_response(),
        Err(_) => not_found(format!("mysql service '{name}' not found")).into_response(),
    }
}

pub async fn my_destroy(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match database::destroy(&state.pool, &state.docker, &name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn my_link(
    State(state): State<SharedState>,
    Path((name, app)): Path<(String, String)>,
) -> impl IntoResponse {
    match database::link(&state.pool, &name, &app, &database::mysql_spec()).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn my_unlink(
    State(state): State<SharedState>,
    Path((name, app)): Path<(String, String)>,
) -> impl IntoResponse {
    match database::unlink(&state.pool, &name, &app).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn my_logs(
    State(state): State<SharedState>,
    Path(name): Path<String>,
    Query(q): Query<LogsQuery>,
) -> impl IntoResponse {
    match database::get_logs(&state.docker, "mysql", &name, q.n.unwrap_or(100)).await {
        Ok(logs) => (StatusCode::OK, Json(serde_json::json!({ "logs": logs }))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

// ── Letsencrypt ───────────────────────────────────────────────────────────────

pub async fn le_enable(
    State(state): State<SharedState>,
    Path(app): Path<String>,
) -> impl IntoResponse {
    match letsencrypt::enable(&state.pool, &state.config, &app).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn le_disable(
    State(state): State<SharedState>,
    Path(app): Path<String>,
) -> impl IntoResponse {
    match letsencrypt::disable(&state.pool, &state.config, &app).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[derive(Deserialize)]
pub struct LeConfigBody {
    pub email: String,
}

pub async fn le_config(
    State(state): State<SharedState>,
    Json(body): Json<LeConfigBody>,
) -> impl IntoResponse {
    match letsencrypt::set_global_email(&state.config, &body.email).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

// ── Networks ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateNetworkBody {
    pub name: String,
}

pub async fn net_list(State(state): State<SharedState>) -> impl IntoResponse {
    match queries::list_networks(&state.pool).await {
        Ok(nets) => {
            let json: Vec<_> = nets
                .iter()
                .map(|n| serde_json::json!({ "id": n.id, "name": n.name }))
                .collect();
            (StatusCode::OK, Json(serde_json::json!(json))).into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn net_create(
    State(state): State<SharedState>,
    Json(body): Json<CreateNetworkBody>,
) -> impl IntoResponse {
    match network::create(&state.pool, &state.docker, &body.name).await {
        Ok(n) => (
            StatusCode::CREATED,
            Json(serde_json::json!({ "id": n.id, "name": n.name })),
        )
            .into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn net_destroy(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match network::destroy(&state.pool, &state.docker, &name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn net_list_for_app(
    State(state): State<SharedState>,
    Path(app): Path<String>,
) -> impl IntoResponse {
    let app_record = match queries::get_app(&state.pool, &app).await {
        Ok(a) => a,
        Err(_) => return not_found(format!("app '{app}' not found")).into_response(),
    };
    match queries::list_app_networks(&state.pool, &app_record.id).await {
        Ok(nets) => {
            let json: Vec<_> = nets
                .iter()
                .map(|n| serde_json::json!({ "id": n.id, "name": n.name }))
                .collect();
            (StatusCode::OK, Json(serde_json::json!(json))).into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn net_attach(
    State(state): State<SharedState>,
    Path((app, net)): Path<(String, String)>,
) -> impl IntoResponse {
    match network::attach(&state.pool, &state.docker, &app, &net).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn net_detach(
    State(state): State<SharedState>,
    Path((app, net)): Path<(String, String)>,
) -> impl IntoResponse {
    match network::detach(&state.pool, &state.docker, &app, &net).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

// ── Storage ───────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct AddMountBody {
    pub host_path: String,
    pub container_path: String,
}

#[derive(Deserialize)]
pub struct EnsureDirBody {
    pub path: String,
}

pub async fn storage_list(
    State(state): State<SharedState>,
    Path(app): Path<String>,
) -> impl IntoResponse {
    let app_record = match queries::get_app(&state.pool, &app).await {
        Ok(a) => a,
        Err(_) => return not_found(format!("app '{app}' not found")).into_response(),
    };
    match storage::list_mounts(&state.pool, &app_record.id).await {
        Ok(mounts) => (StatusCode::OK, Json(serde_json::json!(mounts))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn storage_add(
    State(state): State<SharedState>,
    Path(app): Path<String>,
    Json(body): Json<AddMountBody>,
) -> impl IntoResponse {
    let app_record = match queries::get_app(&state.pool, &app).await {
        Ok(a) => a,
        Err(_) => return not_found(format!("app '{app}' not found")).into_response(),
    };
    match storage::add_mount(&state.pool, &app_record.id, &body.host_path, &body.container_path)
        .await
    {
        Ok(mount) => (StatusCode::CREATED, Json(serde_json::json!(mount))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn storage_remove(
    State(state): State<SharedState>,
    Path((app, id)): Path<(String, String)>,
) -> impl IntoResponse {
    let app_record = match queries::get_app(&state.pool, &app).await {
        Ok(a) => a,
        Err(_) => return not_found(format!("app '{app}' not found")).into_response(),
    };
    match storage::remove_mount(&state.pool, &app_record.id, &id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn storage_ensure(
    State(state): State<SharedState>,
    Path(app): Path<String>,
    Json(body): Json<EnsureDirBody>,
) -> impl IntoResponse {
    // Ensure app exists
    if queries::get_app(&state.pool, &app).await.is_err() {
        return not_found(format!("app '{app}' not found")).into_response();
    }
    match storage::ensure_directory(&body.path) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

// ── Cron ──────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct AddCronBody {
    pub schedule: String,
    pub command: String,
}

pub async fn cron_list(
    State(state): State<SharedState>,
    Path(app): Path<String>,
) -> impl IntoResponse {
    let app_record = match queries::get_app(&state.pool, &app).await {
        Ok(a) => a,
        Err(_) => return not_found(format!("app '{app}' not found")).into_response(),
    };
    match queries::list_cron_entries(&state.pool, &app_record.id).await {
        Ok(entries) => {
            let json: Vec<_> = entries
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "id": e.id,
                        "schedule": e.schedule,
                        "command": e.command,
                    })
                })
                .collect();
            (StatusCode::OK, Json(serde_json::json!(json))).into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn cron_add(
    State(state): State<SharedState>,
    Path(app): Path<String>,
    Json(body): Json<AddCronBody>,
) -> impl IntoResponse {
    let app_record = match queries::get_app(&state.pool, &app).await {
        Ok(a) => a,
        Err(_) => return not_found(format!("app '{app}' not found")).into_response(),
    };
    match queries::add_cron_entry(&state.pool, &app_record.id, &body.schedule, &body.command).await
    {
        Ok(entry) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "id": entry.id,
                "schedule": entry.schedule,
                "command": entry.command,
            })),
        )
            .into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

pub async fn cron_remove(
    State(state): State<SharedState>,
    Path((app, id)): Path<(String, String)>,
) -> impl IntoResponse {
    let app_record = match queries::get_app(&state.pool, &app).await {
        Ok(a) => a,
        Err(_) => return not_found(format!("app '{app}' not found")).into_response(),
    };
    match queries::remove_cron_entry(&state.pool, &app_record.id, &id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}
