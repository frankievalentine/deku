use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use sqlx::SqlitePool;

use super::{internal_error, not_found, SharedState};
use crate::db::queries;
use crate::services::{database, letsencrypt, network, storage};

// ── Shared request/response types ─────────────────────────────────────────────

#[derive(Deserialize, utoipa::ToSchema)]
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
        "container_id": s.container_id,
        "created_at": s.created_at,
    })
}

fn backup_json(backup: &queries::ServiceBackup) -> serde_json::Value {
    serde_json::json!({
        "id": backup.id,
        "service_id": backup.service_id,
        "object_key": backup.object_key,
        "format": backup.format,
        "size_bytes": backup.size_bytes,
        "sha256": backup.sha256,
        "created_at": backup.created_at,
        "restored_at": backup.restored_at,
        "encryption": backup.encryption,
    })
}

async fn linked_apps_json(
    pool: &SqlitePool,
    service_id: &str,
) -> deku_core::error::Result<Vec<serde_json::Value>> {
    let links = queries::list_service_links(pool, service_id).await?;
    let mut linked_apps = Vec::with_capacity(links.len());
    for link in links {
        let app = queries::get_app_by_id(pool, &link.app_id).await?;
        linked_apps.push(serde_json::json!({
            "name": app.name,
            "env_key": link.env_key,
        }));
    }
    Ok(linked_apps)
}

async fn service_info_json(
    pool: &SqlitePool,
    service: &queries::Service,
    spec: &database::DbServiceSpec,
) -> anyhow::Result<serde_json::Value> {
    let connection = database::connection_info(spec, service)?;
    let linked_apps = linked_apps_json(pool, &service.id).await?;
    Ok(serde_json::json!({
        "id": service.id,
        "name": service.name,
        "status": service.status,
        "plugin": service.plugin,
        "container_id": service.container_id,
        "created_at": service.created_at,
        "connection": connection,
        "links": linked_apps,
    }))
}

// ── Generic service routes ────────────────────────────────────────────────────

/// Resolve `{type}` to a spec, or report an unknown type.
///
/// Returning `Response` by value keeps each call site to one line; the size cost
/// is irrelevant on this error path.
#[allow(clippy::result_large_err)]
fn spec_or_404(plugin: &str) -> Result<database::DbServiceSpec, axum::response::Response> {
    database::spec_for(plugin).ok_or_else(|| {
        not_found(format!(
            "unknown service type '{plugin}'; expected postgres, redis, mysql, mariadb, or mongodb"
        ))
        .into_response()
    })
}

#[utoipa::path(
    get,
    path = "/api/services/{type}",
    tag = "services",
    params(("type" = String, Path, description = "Service type")),
    responses((status = 200, description = "List services of a type"), (status = 404, description = "Unknown service type"))
)]
pub async fn svc_list(
    State(state): State<SharedState>,
    Path(service_type): Path<String>,
) -> impl IntoResponse {
    if let Err(response) = spec_or_404(&service_type) {
        return response;
    }
    match queries::list_services(&state.pool, &service_type).await {
        Ok(services) => {
            let json: Vec<_> = services.iter().map(service_json).collect();
            (StatusCode::OK, Json(serde_json::json!(json))).into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/services/{type}",
    tag = "services",
    params(("type" = String, Path, description = "Service type")),
    request_body = CreateServiceBody,
    responses((status = 201, description = "Service created"), (status = 404, description = "Unknown service type"))
)]
pub async fn svc_create(
    State(state): State<SharedState>,
    Path(service_type): Path<String>,
    Json(body): Json<CreateServiceBody>,
) -> impl IntoResponse {
    let spec = match spec_or_404(&service_type) {
        Ok(spec) => spec,
        Err(response) => return response,
    };
    match database::create(&state.pool, &state.docker, spec, &body.name).await {
        Ok(svc) => (StatusCode::CREATED, Json(service_json(&svc))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/services/{type}/{name}",
    tag = "services",
    params(
        ("type" = String, Path, description = "Service type"),
        ("name" = String, Path, description = "Service name")
    ),
    responses((status = 200, description = "Show a service"), (status = 404, description = "Not found"))
)]
pub async fn svc_info(
    State(state): State<SharedState>,
    Path((service_type, name)): Path<(String, String)>,
) -> impl IntoResponse {
    let spec = match spec_or_404(&service_type) {
        Ok(spec) => spec,
        Err(response) => return response,
    };
    match queries::get_service_for_plugin(&state.pool, &name, &service_type).await {
        Ok(svc) => match service_info_json(&state.pool, &svc, &spec).await {
            Ok(json) => (StatusCode::OK, Json(json)).into_response(),
            Err(e) => internal_error(e).into_response(),
        },
        Err(_) => not_found(format!("{service_type} service '{name}' not found")).into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/api/services/{type}/{name}",
    tag = "services",
    params(
        ("type" = String, Path, description = "Service type"),
        ("name" = String, Path, description = "Service name")
    ),
    responses((status = 204, description = "No content"), (status = 404, description = "Unknown service type"))
)]
pub async fn svc_destroy(
    State(state): State<SharedState>,
    Path((service_type, name)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(response) = spec_or_404(&service_type) {
        return response;
    }
    match database::destroy(&state.pool, &state.docker, &name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/services/{type}/{name}/link/{app}",
    tag = "services",
    params(
        ("type" = String, Path, description = "Service type"),
        ("name" = String, Path, description = "Service name"),
        ("app" = String, Path, description = "App name")
    ),
    responses((status = 204, description = "No content"), (status = 404, description = "Unknown service type"))
)]
pub async fn svc_link(
    State(state): State<SharedState>,
    Path((service_type, name, app)): Path<(String, String, String)>,
) -> impl IntoResponse {
    let spec = match spec_or_404(&service_type) {
        Ok(spec) => spec,
        Err(response) => return response,
    };
    match database::link(&state.pool, &state.config, &name, &app, &spec).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/api/services/{type}/{name}/link/{app}",
    tag = "services",
    params(
        ("type" = String, Path, description = "Service type"),
        ("name" = String, Path, description = "Service name"),
        ("app" = String, Path, description = "App name")
    ),
    responses((status = 204, description = "No content"), (status = 404, description = "Unknown service type"))
)]
pub async fn svc_unlink(
    State(state): State<SharedState>,
    Path((service_type, name, app)): Path<(String, String, String)>,
) -> impl IntoResponse {
    if let Err(response) = spec_or_404(&service_type) {
        return response;
    }
    match database::unlink(&state.pool, &name, &app).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/services/{type}/{name}/logs",
    tag = "services",
    params(
        ("type" = String, Path, description = "Service type"),
        ("name" = String, Path, description = "Service name")
    ),
    responses((status = 200, description = "Service logs"), (status = 404, description = "Unknown service type"))
)]
pub async fn svc_logs(
    State(state): State<SharedState>,
    Path((service_type, name)): Path<(String, String)>,
    Query(q): Query<LogsQuery>,
) -> impl IntoResponse {
    if let Err(response) = spec_or_404(&service_type) {
        return response;
    }
    match database::get_logs(&state.docker, &service_type, &name, q.n.unwrap_or(100)).await {
        Ok(logs) => (StatusCode::OK, Json(serde_json::json!({ "logs": logs }))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/services/{type}/{name}/backups",
    tag = "services",
    params(
        ("type" = String, Path, description = "Service type"),
        ("name" = String, Path, description = "Service name")
    ),
    responses((status = 200, description = "Service backups"), (status = 404, description = "Unknown service type"))
)]
pub async fn svc_backups(
    State(state): State<SharedState>,
    Path((service_type, name)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(response) = spec_or_404(&service_type) {
        return response;
    }
    match database::list_backups(&state.pool, &name, &service_type).await {
        Ok(backups) => {
            let json: Vec<_> = backups.iter().map(backup_json).collect();
            (StatusCode::OK, Json(serde_json::json!(json))).into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/services/{type}/{name}/backups",
    tag = "services",
    params(
        ("type" = String, Path, description = "Service type"),
        ("name" = String, Path, description = "Service name")
    ),
    responses((status = 201, description = "Backup created"), (status = 404, description = "Unknown service type"))
)]
pub async fn svc_backup(
    State(state): State<SharedState>,
    Path((service_type, name)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(response) = spec_or_404(&service_type) {
        return response;
    }
    let cfg = match crate::config::load() {
        Ok(cfg) => cfg,
        Err(e) => return internal_error(e).into_response(),
    };
    match database::backup_for_plugin(&state.pool, &state.docker, &cfg, &service_type, &name).await
    {
        Ok(backup) => (StatusCode::CREATED, Json(backup_json(&backup))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/services/{type}/{name}/restore/{backup_id}",
    tag = "services",
    params(
        ("type" = String, Path, description = "Service type"),
        ("name" = String, Path, description = "Service name"),
        ("backup_id" = String, Path, description = "Backup id")
    ),
    responses((status = 200, description = "Backup restored"), (status = 404, description = "Unknown service type"))
)]
pub async fn svc_restore(
    State(state): State<SharedState>,
    Path((service_type, name, backup_id)): Path<(String, String, String)>,
) -> impl IntoResponse {
    if let Err(response) = spec_or_404(&service_type) {
        return response;
    }
    let cfg = match crate::config::load() {
        Ok(cfg) => cfg,
        Err(e) => return internal_error(e).into_response(),
    };
    match database::restore_for_plugin(
        &state.pool,
        &state.docker,
        &cfg,
        &service_type,
        &name,
        &backup_id,
    )
    .await
    {
        Ok(backup) => (StatusCode::OK, Json(backup_json(&backup))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

// ── Postgres ──────────────────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/postgres/services",
    tag = "services",
    responses((status = 200, description = "List Postgres services"))
)]
pub async fn pg_list(State(state): State<SharedState>) -> impl IntoResponse {
    match queries::list_services(&state.pool, "postgres").await {
        Ok(services) => {
            let json: Vec<_> = services.iter().map(service_json).collect();
            (StatusCode::OK, Json(serde_json::json!(json))).into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/postgres/services",
    tag = "services",
    request_body = CreateServiceBody,
    responses((status = 201, description = "Create a Postgres service"))
)]
pub async fn pg_create(
    State(state): State<SharedState>,
    Json(body): Json<CreateServiceBody>,
) -> impl IntoResponse {
    match database::create(
        &state.pool,
        &state.docker,
        database::postgres_spec(),
        &body.name,
    )
    .await
    {
        Ok(svc) => (StatusCode::CREATED, Json(service_json(&svc))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/postgres/services/{name}",
    tag = "services",
    params(("name" = String, Path, description = "Service name")),
    responses((status = 200, description = "Show a Postgres service"))
)]
pub async fn pg_info(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match queries::get_service_for_plugin(&state.pool, &name, "postgres").await {
        Ok(svc) => match service_info_json(&state.pool, &svc, &database::postgres_spec()).await {
            Ok(json) => (StatusCode::OK, Json(json)).into_response(),
            Err(e) => internal_error(e).into_response(),
        },
        Err(_) => not_found(format!("postgres service '{name}' not found")).into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/api/postgres/services/{name}",
    tag = "services",
    params(("name" = String, Path, description = "Service name")),
    responses((status = 204, description = "No content"))
)]
pub async fn pg_destroy(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match database::destroy(&state.pool, &state.docker, &name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/postgres/services/{name}/link/{app}",
    tag = "services",
    params(("name" = String, Path, description = "Service name"), ("app" = String, Path, description = "App name")),
    responses((status = 204, description = "No content"))
)]
pub async fn pg_link(
    State(state): State<SharedState>,
    Path((name, app)): Path<(String, String)>,
) -> impl IntoResponse {
    match database::link(
        &state.pool,
        &state.config,
        &name,
        &app,
        &database::postgres_spec(),
    )
    .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/api/postgres/services/{name}/link/{app}",
    tag = "services",
    params(("name" = String, Path, description = "Service name"), ("app" = String, Path, description = "App name")),
    responses((status = 204, description = "No content"))
)]
pub async fn pg_unlink(
    State(state): State<SharedState>,
    Path((name, app)): Path<(String, String)>,
) -> impl IntoResponse {
    match database::unlink(&state.pool, &name, &app).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/postgres/services/{name}/logs",
    tag = "services",
    params(("name" = String, Path, description = "Service name")),
    responses((status = 200, description = "Stream Postgres logs"))
)]
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

#[utoipa::path(
    get,
    path = "/api/postgres/services/{name}/backups",
    tag = "services",
    params(("name" = String, Path, description = "Service name")),
    responses((status = 200, description = "List Postgres backups"))
)]
pub async fn pg_backups(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match database::list_backups(&state.pool, &name, "postgres").await {
        Ok(backups) => {
            let json: Vec<_> = backups.iter().map(backup_json).collect();
            (StatusCode::OK, Json(serde_json::json!(json))).into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/postgres/services/{name}/backups",
    tag = "services",
    params(("name" = String, Path, description = "Service name")),
    responses((status = 201, description = "Create a Postgres backup"))
)]
pub async fn pg_backup(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match crate::config::load() {
        Ok(cfg) => match database::backup_postgres(&state.pool, &state.docker, &cfg, &name).await {
            Ok(backup) => (StatusCode::CREATED, Json(backup_json(&backup))).into_response(),
            Err(e) => internal_error(e).into_response(),
        },
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/postgres/services/{name}/restore/{backup_id}",
    tag = "services",
    params(("name" = String, Path, description = "Service name"), ("backup_id" = String, Path, description = "Backup id")),
    responses((status = 200, description = "Restore a Postgres backup"))
)]
pub async fn pg_restore(
    State(state): State<SharedState>,
    Path((name, backup_id)): Path<(String, String)>,
) -> impl IntoResponse {
    match crate::config::load() {
        Ok(cfg) => {
            match database::restore_postgres(&state.pool, &state.docker, &cfg, &name, &backup_id)
                .await
            {
                Ok(backup) => (StatusCode::OK, Json(backup_json(&backup))).into_response(),
                Err(e) => internal_error(e).into_response(),
            }
        }
        Err(e) => internal_error(e).into_response(),
    }
}

// ── Redis ─────────────────────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/redis/services",
    tag = "services",
    responses((status = 200, description = "List Redis services"))
)]
pub async fn rd_list(State(state): State<SharedState>) -> impl IntoResponse {
    match queries::list_services(&state.pool, "redis").await {
        Ok(services) => {
            let json: Vec<_> = services.iter().map(service_json).collect();
            (StatusCode::OK, Json(serde_json::json!(json))).into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/redis/services",
    tag = "services",
    request_body = CreateServiceBody,
    responses((status = 201, description = "Create a Redis service"))
)]
pub async fn rd_create(
    State(state): State<SharedState>,
    Json(body): Json<CreateServiceBody>,
) -> impl IntoResponse {
    match database::create(
        &state.pool,
        &state.docker,
        database::redis_spec(),
        &body.name,
    )
    .await
    {
        Ok(svc) => (StatusCode::CREATED, Json(service_json(&svc))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/redis/services/{name}",
    tag = "services",
    params(("name" = String, Path, description = "Service name")),
    responses((status = 200, description = "Show a Redis service"))
)]
pub async fn rd_info(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match queries::get_service_for_plugin(&state.pool, &name, "redis").await {
        Ok(svc) => match service_info_json(&state.pool, &svc, &database::redis_spec()).await {
            Ok(json) => (StatusCode::OK, Json(json)).into_response(),
            Err(e) => internal_error(e).into_response(),
        },
        Err(_) => not_found(format!("redis service '{name}' not found")).into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/api/redis/services/{name}",
    tag = "services",
    params(("name" = String, Path, description = "Service name")),
    responses((status = 204, description = "No content"))
)]
pub async fn rd_destroy(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match database::destroy(&state.pool, &state.docker, &name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/redis/services/{name}/link/{app}",
    tag = "services",
    params(("name" = String, Path, description = "Service name"), ("app" = String, Path, description = "App name")),
    responses((status = 204, description = "No content"))
)]
pub async fn rd_link(
    State(state): State<SharedState>,
    Path((name, app)): Path<(String, String)>,
) -> impl IntoResponse {
    match database::link(
        &state.pool,
        &state.config,
        &name,
        &app,
        &database::redis_spec(),
    )
    .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/api/redis/services/{name}/link/{app}",
    tag = "services",
    params(("name" = String, Path, description = "Service name"), ("app" = String, Path, description = "App name")),
    responses((status = 204, description = "No content"))
)]
pub async fn rd_unlink(
    State(state): State<SharedState>,
    Path((name, app)): Path<(String, String)>,
) -> impl IntoResponse {
    match database::unlink(&state.pool, &name, &app).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/redis/services/{name}/logs",
    tag = "services",
    params(("name" = String, Path, description = "Service name")),
    responses((status = 200, description = "Stream Redis logs"))
)]
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

#[utoipa::path(
    get,
    path = "/api/redis/services/{name}/backups",
    tag = "services",
    params(("name" = String, Path, description = "Service name")),
    responses((status = 200, description = "List Redis backups"))
)]
pub async fn rd_backups(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match database::list_backups(&state.pool, &name, "redis").await {
        Ok(backups) => {
            let json: Vec<_> = backups.iter().map(backup_json).collect();
            (StatusCode::OK, Json(serde_json::json!(json))).into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/redis/services/{name}/backups",
    tag = "services",
    params(("name" = String, Path, description = "Service name")),
    responses((status = 201, description = "Create a Redis backup"))
)]
pub async fn rd_backup(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match crate::config::load() {
        Ok(cfg) => match database::backup_redis(&state.pool, &state.docker, &cfg, &name).await {
            Ok(backup) => (StatusCode::CREATED, Json(backup_json(&backup))).into_response(),
            Err(e) => internal_error(e).into_response(),
        },
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/redis/services/{name}/restore/{backup_id}",
    tag = "services",
    params(("name" = String, Path, description = "Service name"), ("backup_id" = String, Path, description = "Backup id")),
    responses((status = 200, description = "Restore a Redis backup"))
)]
pub async fn rd_restore(
    State(state): State<SharedState>,
    Path((name, backup_id)): Path<(String, String)>,
) -> impl IntoResponse {
    match crate::config::load() {
        Ok(cfg) => {
            match database::restore_redis(&state.pool, &state.docker, &cfg, &name, &backup_id).await
            {
                Ok(backup) => (StatusCode::OK, Json(backup_json(&backup))).into_response(),
                Err(e) => internal_error(e).into_response(),
            }
        }
        Err(e) => internal_error(e).into_response(),
    }
}

// ── MySQL ─────────────────────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/mysql/services",
    tag = "services",
    responses((status = 200, description = "List MySQL services"))
)]
pub async fn my_list(State(state): State<SharedState>) -> impl IntoResponse {
    match queries::list_services(&state.pool, "mysql").await {
        Ok(services) => {
            let json: Vec<_> = services.iter().map(service_json).collect();
            (StatusCode::OK, Json(serde_json::json!(json))).into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/mysql/services",
    tag = "services",
    request_body = CreateServiceBody,
    responses((status = 201, description = "Create a MySQL service"))
)]
pub async fn my_create(
    State(state): State<SharedState>,
    Json(body): Json<CreateServiceBody>,
) -> impl IntoResponse {
    match database::create(
        &state.pool,
        &state.docker,
        database::mysql_spec(),
        &body.name,
    )
    .await
    {
        Ok(svc) => (StatusCode::CREATED, Json(service_json(&svc))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/mysql/services/{name}",
    tag = "services",
    params(("name" = String, Path, description = "Service name")),
    responses((status = 200, description = "Show a MySQL service"))
)]
pub async fn my_info(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match queries::get_service_for_plugin(&state.pool, &name, "mysql").await {
        Ok(svc) => match service_info_json(&state.pool, &svc, &database::mysql_spec()).await {
            Ok(json) => (StatusCode::OK, Json(json)).into_response(),
            Err(e) => internal_error(e).into_response(),
        },
        Err(_) => not_found(format!("mysql service '{name}' not found")).into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/api/mysql/services/{name}",
    tag = "services",
    params(("name" = String, Path, description = "Service name")),
    responses((status = 204, description = "No content"))
)]
pub async fn my_destroy(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match database::destroy(&state.pool, &state.docker, &name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/mysql/services/{name}/link/{app}",
    tag = "services",
    params(("name" = String, Path, description = "Service name"), ("app" = String, Path, description = "App name")),
    responses((status = 204, description = "No content"))
)]
pub async fn my_link(
    State(state): State<SharedState>,
    Path((name, app)): Path<(String, String)>,
) -> impl IntoResponse {
    match database::link(
        &state.pool,
        &state.config,
        &name,
        &app,
        &database::mysql_spec(),
    )
    .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/api/mysql/services/{name}/link/{app}",
    tag = "services",
    params(("name" = String, Path, description = "Service name"), ("app" = String, Path, description = "App name")),
    responses((status = 204, description = "No content"))
)]
pub async fn my_unlink(
    State(state): State<SharedState>,
    Path((name, app)): Path<(String, String)>,
) -> impl IntoResponse {
    match database::unlink(&state.pool, &name, &app).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/mysql/services/{name}/logs",
    tag = "services",
    params(("name" = String, Path, description = "Service name")),
    responses((status = 200, description = "Stream MySQL logs"))
)]
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

#[utoipa::path(
    get,
    path = "/api/mysql/services/{name}/backups",
    tag = "services",
    params(("name" = String, Path, description = "Service name")),
    responses((status = 200, description = "List MySQL backups"))
)]
pub async fn my_backups(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match database::list_backups(&state.pool, &name, "mysql").await {
        Ok(backups) => {
            let json: Vec<_> = backups.iter().map(backup_json).collect();
            (StatusCode::OK, Json(serde_json::json!(json))).into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/mysql/services/{name}/backups",
    tag = "services",
    params(("name" = String, Path, description = "Service name")),
    responses((status = 201, description = "Create a MySQL backup"))
)]
pub async fn my_backup(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match crate::config::load() {
        Ok(cfg) => match database::backup_mysql(&state.pool, &state.docker, &cfg, &name).await {
            Ok(backup) => (StatusCode::CREATED, Json(backup_json(&backup))).into_response(),
            Err(e) => internal_error(e).into_response(),
        },
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/mysql/services/{name}/restore/{backup_id}",
    tag = "services",
    params(("name" = String, Path, description = "Service name"), ("backup_id" = String, Path, description = "Backup id")),
    responses((status = 200, description = "Restore a MySQL backup"))
)]
pub async fn my_restore(
    State(state): State<SharedState>,
    Path((name, backup_id)): Path<(String, String)>,
) -> impl IntoResponse {
    match crate::config::load() {
        Ok(cfg) => {
            match database::restore_mysql(&state.pool, &state.docker, &cfg, &name, &backup_id).await
            {
                Ok(backup) => (StatusCode::OK, Json(backup_json(&backup))).into_response(),
                Err(e) => internal_error(e).into_response(),
            }
        }
        Err(e) => internal_error(e).into_response(),
    }
}

// ── Letsencrypt ───────────────────────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/api/letsencrypt/enable/{app}",
    tag = "letsencrypt",
    params(("app" = String, Path, description = "App name")),
    responses((status = 204, description = "No content"))
)]
pub async fn le_enable(
    State(state): State<SharedState>,
    Path(app): Path<String>,
) -> impl IntoResponse {
    match letsencrypt::enable(&state.pool, &state.config, &app).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/letsencrypt/disable/{app}",
    tag = "letsencrypt",
    params(("app" = String, Path, description = "App name")),
    responses((status = 204, description = "No content"))
)]
pub async fn le_disable(
    State(state): State<SharedState>,
    Path(app): Path<String>,
) -> impl IntoResponse {
    match letsencrypt::disable(&state.pool, &state.config, &app).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/letsencrypt/status/{app}",
    tag = "letsencrypt",
    params(("app" = String, Path, description = "App name")),
    responses((status = 200, description = "Let's Encrypt status for an app"))
)]
pub async fn le_status(
    State(state): State<SharedState>,
    Path(app): Path<String>,
) -> impl IntoResponse {
    match letsencrypt::status(&state.pool, &app).await {
        Ok(status) => (StatusCode::OK, Json(serde_json::json!(status))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct LeConfigBody {
    pub email: String,
}

#[utoipa::path(
    get,
    path = "/api/letsencrypt/config",
    tag = "letsencrypt",
    responses((status = 200, description = "Show Let's Encrypt configuration"))
)]
pub async fn le_get_config(State(state): State<SharedState>) -> impl IntoResponse {
    match letsencrypt::get_global_email(&state.config).await {
        Ok(email) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "configured": email.is_some(),
                "email": email,
            })),
        )
            .into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/letsencrypt/config",
    tag = "letsencrypt",
    request_body = LeConfigBody,
    responses((status = 204, description = "No content"))
)]
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

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateNetworkBody {
    pub name: String,
}

#[utoipa::path(
    get,
    path = "/api/networks",
    tag = "networks",
    responses((status = 200, description = "List networks"))
)]
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

#[utoipa::path(
    post,
    path = "/api/networks",
    tag = "networks",
    request_body = CreateNetworkBody,
    responses((status = 201, description = "Create a network"))
)]
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

#[utoipa::path(
    delete,
    path = "/api/networks/{name}",
    tag = "networks",
    params(("name" = String, Path, description = "Network name")),
    responses((status = 204, description = "No content"))
)]
pub async fn net_destroy(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match network::destroy(&state.pool, &state.docker, &name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/apps/{app}/networks",
    tag = "networks",
    params(("app" = String, Path, description = "App name")),
    responses((status = 200, description = "List networks attached to an app"))
)]
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

#[utoipa::path(
    post,
    path = "/api/apps/{app}/networks/{network}",
    tag = "networks",
    params(("app" = String, Path, description = "App name"), ("network" = String, Path, description = "Network name")),
    responses((status = 204, description = "No content"))
)]
pub async fn net_attach(
    State(state): State<SharedState>,
    Path((app, net)): Path<(String, String)>,
) -> impl IntoResponse {
    match network::attach(&state.pool, &state.docker, &app, &net).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/api/apps/{app}/networks/{network}",
    tag = "networks",
    params(("app" = String, Path, description = "App name"), ("network" = String, Path, description = "Network name")),
    responses((status = 204, description = "No content"))
)]
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

#[derive(Deserialize, utoipa::ToSchema)]
pub struct AddMountBody {
    pub host_path: String,
    pub container_path: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct EnsureDirBody {
    pub path: String,
}

#[utoipa::path(
    get,
    path = "/api/apps/{app}/storage",
    tag = "storage",
    params(("app" = String, Path, description = "App name")),
    responses((status = 200, description = "List storage mounts for an app"))
)]
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

#[utoipa::path(
    post,
    path = "/api/apps/{app}/storage",
    tag = "storage",
    params(("app" = String, Path, description = "App name")),
    request_body = AddMountBody,
    responses((status = 201, description = "Add a storage mount"))
)]
pub async fn storage_add(
    State(state): State<SharedState>,
    Path(app): Path<String>,
    Json(body): Json<AddMountBody>,
) -> impl IntoResponse {
    let app_record = match queries::get_app(&state.pool, &app).await {
        Ok(a) => a,
        Err(_) => return not_found(format!("app '{app}' not found")).into_response(),
    };
    match storage::add_mount(
        &state.pool,
        &app_record.id,
        &body.host_path,
        &body.container_path,
    )
    .await
    {
        Ok(mount) => (StatusCode::CREATED, Json(serde_json::json!(mount))).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/api/apps/{app}/storage/{id}",
    tag = "storage",
    params(("app" = String, Path, description = "App name"), ("id" = String, Path, description = "Mount id")),
    responses((status = 204, description = "No content"))
)]
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

#[utoipa::path(
    post,
    path = "/api/apps/{app}/storage/ensure",
    tag = "storage",
    params(("app" = String, Path, description = "App name")),
    request_body = EnsureDirBody,
    responses((status = 204, description = "No content"))
)]
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

#[derive(Deserialize, utoipa::ToSchema)]
pub struct AddCronBody {
    pub schedule: String,
    pub command: String,
}

#[utoipa::path(
    get,
    path = "/api/apps/{app}/cron",
    tag = "cron",
    params(("app" = String, Path, description = "App name")),
    responses((status = 200, description = "List cron jobs"))
)]
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

#[utoipa::path(
    post,
    path = "/api/apps/{app}/cron",
    tag = "cron",
    params(("app" = String, Path, description = "App name")),
    request_body = AddCronBody,
    responses((status = 201, description = "Add a cron job"))
)]
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

#[utoipa::path(
    delete,
    path = "/api/apps/{app}/cron/{id}",
    tag = "cron",
    params(("app" = String, Path, description = "App name"), ("id" = String, Path, description = "Cron job id")),
    responses((status = 204, description = "No content"))
)]
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

// ── Backup schedules ──────────────────────────────────────────────────────────

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SetBackupScheduleBody {
    /// Hours between backups.
    pub interval_hours: i64,
    /// Number of backups to keep; older ones are pruned.
    pub retention: i64,
}

fn schedule_json(
    service: &queries::Service,
    schedule: Option<&queries::BackupSchedule>,
) -> serde_json::Value {
    match schedule {
        Some(schedule) => serde_json::json!({
            "service": service.name,
            "plugin": service.plugin,
            "enabled": schedule.enabled != 0,
            "interval_hours": schedule.interval_hours,
            "retention": schedule.retention,
            "last_run_at": schedule.last_run_at,
            "last_status": schedule.last_status,
            "next_run_at": schedule.next_run_at,
        }),
        None => serde_json::json!({
            "service": service.name,
            "plugin": service.plugin,
            "enabled": false,
        }),
    }
}

#[utoipa::path(
    get,
    path = "/api/services/{name}/backup-schedule",
    tag = "backups",
    params(("name" = String, Path, description = "Service name")),
    responses((status = 200, description = "Show a service backup schedule"))
)]
pub async fn get_backup_schedule(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let service = match queries::get_service(&state.pool, &name).await {
        Ok(service) => service,
        Err(_) => return not_found(format!("service '{name}' not found")).into_response(),
    };
    match queries::get_backup_schedule(&state.pool, &service.id).await {
        Ok(schedule) => (
            StatusCode::OK,
            Json(schedule_json(&service, schedule.as_ref())),
        )
            .into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/services/{name}/backup-schedule",
    tag = "backups",
    params(("name" = String, Path, description = "Service name")),
    request_body = SetBackupScheduleBody,
    responses((status = 200, description = "Set a service backup schedule"))
)]
pub async fn set_backup_schedule(
    State(state): State<SharedState>,
    Path(name): Path<String>,
    Json(body): Json<SetBackupScheduleBody>,
) -> impl IntoResponse {
    let service = match queries::get_service(&state.pool, &name).await {
        Ok(service) => service,
        Err(_) => return not_found(format!("service '{name}' not found")).into_response(),
    };
    if body.interval_hours < 1 || body.retention < 1 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "interval_hours and retention must both be at least 1"
            })),
        )
            .into_response();
    }
    if let Err(e) = queries::upsert_backup_schedule(
        &state.pool,
        &service.id,
        body.interval_hours,
        body.retention,
    )
    .await
    {
        return internal_error(e).into_response();
    }
    match queries::get_backup_schedule(&state.pool, &service.id).await {
        Ok(schedule) => (
            StatusCode::OK,
            Json(schedule_json(&service, schedule.as_ref())),
        )
            .into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/api/services/{name}/backup-schedule",
    tag = "backups",
    params(("name" = String, Path, description = "Service name")),
    responses((status = 204, description = "No content"))
)]
pub async fn delete_backup_schedule(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let service = match queries::get_service(&state.pool, &name).await {
        Ok(service) => service,
        Err(_) => return not_found(format!("service '{name}' not found")).into_response(),
    };
    match queries::delete_backup_schedule(&state.pool, &service.id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/backup-schedules",
    tag = "backups",
    responses((status = 200, description = "List backup schedules"))
)]
pub async fn list_backup_schedules(State(state): State<SharedState>) -> impl IntoResponse {
    let schedules = match queries::list_backup_schedules(&state.pool).await {
        Ok(schedules) => schedules,
        Err(e) => return internal_error(e).into_response(),
    };

    let mut out = Vec::with_capacity(schedules.len());
    for schedule in schedules {
        let name = queries::get_service_by_id(&state.pool, &schedule.service_id)
            .await
            .map(|service| service.name)
            .unwrap_or_else(|_| schedule.service_id.clone());
        out.push(serde_json::json!({
            "service": name,
            "enabled": schedule.enabled != 0,
            "interval_hours": schedule.interval_hours,
            "retention": schedule.retention,
            "last_run_at": schedule.last_run_at,
            "last_status": schedule.last_status,
            "next_run_at": schedule.next_run_at,
        }));
    }

    (StatusCode::OK, Json(serde_json::json!(out))).into_response()
}
