//! Streaming API handlers for `deku run` and `deku exec`.

use std::convert::Infallible;

use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::{extract::Path, extract::State, http::StatusCode, Json};
use serde::Deserialize;
use tokio::sync::mpsc::unbounded_channel;
use tokio_stream::wrappers::UnboundedReceiverStream;

use super::SharedState;
use crate::console::{self, OutputStream};
use crate::db::queries;

#[derive(Debug, Deserialize)]
pub struct ConsoleBody {
    /// Command and arguments, e.g. `["python", "manage.py", "migrate"]`.
    pub command: Vec<String>,
}

fn validate_command(command: &[String]) -> Option<Response> {
    if command.is_empty() {
        return Some(
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "command must not be empty" })),
            )
                .into_response(),
        );
    }
    if command.len() > 128 || command.iter().any(|part| part.len() > 4096) {
        return Some(
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "command is too large" })),
            )
                .into_response(),
        );
    }
    None
}

fn emit(
    tx: &tokio::sync::mpsc::UnboundedSender<Result<Event, Infallible>>,
    stream: OutputStream,
    chunk: &str,
) {
    for line in chunk.split_inclusive('\n') {
        let payload = serde_json::json!({ "stream": stream.as_str(), "line": line });
        let _ = tx.send(Ok(Event::default().data(payload.to_string())));
    }
}

fn finish(tx: &tokio::sync::mpsc::UnboundedSender<Result<Event, Infallible>>, exit_code: i32) {
    let payload = serde_json::json!({ "exit_code": exit_code });
    let _ = tx.send(Ok(Event::default().data(payload.to_string())));
}

fn sse(rx: tokio::sync::mpsc::UnboundedReceiver<Result<Event, Infallible>>) -> Response {
    Sse::new(UnboundedReceiverStream::new(rx))
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Run a one-off command in a fresh container built from the app's image.
#[utoipa::path(
    post,
    path = "/api/apps/{name}/run",
    tag = "console",
    params(("name" = String, Path, description = "App name")),
    request_body = super::openapi::ConsoleCommandSchema,
    responses(
        (status = 200, description = "Server-sent stream of `{stream,line}` frames ending with `{exit_code}`"),
        (status = 404, description = "App not found"),
        (status = 409, description = "App has no deployed image")
    )
)]
pub async fn run(
    State(state): State<SharedState>,
    Path(name): Path<String>,
    Json(body): Json<ConsoleBody>,
) -> Response {
    if let Some(response) = validate_command(&body.command) {
        return response;
    }

    let app = match queries::get_app(&state.pool, &name).await {
        Ok(app) => app,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return super::not_found(format!("app '{name}' not found")).into_response();
        }
        Err(error) => return super::internal_error(error).into_response(),
    };

    let image = match queries::get_latest_deployment(&state.pool, &app.id).await {
        Ok(Some(deployment)) if deployment.image_tag.is_some() => {
            deployment.image_tag.unwrap_or_default()
        }
        Ok(_) => {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({ "error": "app has no deployed image yet" })),
            )
                .into_response();
        }
        Err(error) => return super::internal_error(error).into_response(),
    };

    let env: Vec<String> =
        match crate::secrets::get_config_vars(&state.pool, &state.config, &app.id).await {
            Ok(vars) => vars
                .iter()
                .map(|var| format!("{}={}", var.key, var.value))
                .collect(),
            Err(error) => return super::internal_error(error).into_response(),
        };
    let volumes: Vec<(String, String)> =
        match queries::list_storage_mounts(&state.pool, &app.id).await {
            Ok(mounts) => mounts
                .iter()
                .map(|mount| (mount.host_path.clone(), mount.container_path.clone()))
                .collect(),
            Err(error) => return super::internal_error(error).into_response(),
        };

    let container_name = format!(
        "deku.{}.run.{}",
        app.name,
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    );
    let docker = state.docker.clone();
    let command = body.command;

    let (tx, rx) = unbounded_channel();
    let output_tx = tx.clone();
    tokio::spawn(async move {
        let result = console::run_once(
            &docker,
            &container_name,
            &image,
            &env,
            &volumes,
            &command,
            |stream, chunk| emit(&output_tx, stream, chunk),
        )
        .await;
        let exit_code = match result {
            Ok(code) => code,
            Err(error) => {
                emit(&tx, OutputStream::Stderr, &format!("error: {error}\n"));
                1
            }
        };
        finish(&tx, exit_code);
    });

    sse(rx)
}

/// Run a command inside an already-running app container.
#[utoipa::path(
    post,
    path = "/api/apps/{name}/exec",
    tag = "console",
    params(("name" = String, Path, description = "App name")),
    request_body = super::openapi::ConsoleCommandSchema,
    responses(
        (status = 200, description = "Server-sent stream of `{stream,line}` frames ending with `{exit_code}`"),
        (status = 404, description = "App not found"),
        (status = 409, description = "App has no running containers")
    )
)]
pub async fn exec(
    State(state): State<SharedState>,
    Path(name): Path<String>,
    Json(body): Json<ConsoleBody>,
) -> Response {
    if let Some(response) = validate_command(&body.command) {
        return response;
    }

    let app = match queries::get_app(&state.pool, &name).await {
        Ok(app) => app,
        Err(deku_core::error::DekuError::AppNotFound(_)) => {
            return super::not_found(format!("app '{name}' not found")).into_response();
        }
        Err(error) => return super::internal_error(error).into_response(),
    };

    let containers = match queries::list_containers_for_app(&state.pool, &app.id).await {
        Ok(containers) => containers,
        Err(error) => return super::internal_error(error).into_response(),
    };
    let target = containers
        .iter()
        .find(|container| container.process_type == "web")
        .or_else(|| containers.first())
        .map(|container| container.id.clone());

    let container_id = match target {
        Some(id) => id,
        None => {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({ "error": "app has no running containers" })),
            )
                .into_response();
        }
    };

    let docker = state.docker.clone();
    let command = body.command;

    let (tx, rx) = unbounded_channel();
    let output_tx = tx.clone();
    tokio::spawn(async move {
        let result = console::exec_in(&docker, &container_id, &command, |stream, chunk| {
            emit(&output_tx, stream, chunk)
        })
        .await;
        let exit_code = match result {
            Ok(code) => code,
            Err(error) => {
                emit(&tx, OutputStream::Stderr, &format!("error: {error}\n"));
                1
            }
        };
        finish(&tx, exit_code);
    });

    sse(rx)
}

#[cfg(test)]
mod tests {
    use super::validate_command;

    #[test]
    fn validate_command_rejects_empty_and_oversized() {
        assert!(validate_command(&[]).is_some());
        assert!(validate_command(&["sh".to_string(), "-c".to_string()]).is_none());
        assert!(validate_command(&vec!["x".to_string(); 129]).is_some());
        assert!(validate_command(&["x".repeat(4097)]).is_some());
    }
}
