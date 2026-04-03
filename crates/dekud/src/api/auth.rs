use axum::{extract::Request, http::StatusCode, middleware::Next, response::IntoResponse, Json};
use deku_core::auth::verify_dashboard_token;

use super::SharedState;

/// Axum middleware that validates the dashboard bearer token on TCP requests.
/// Apply only to the TCP router — the Unix socket router is trusted local access.
pub async fn require_auth(
    axum::extract::State(state): axum::extract::State<SharedState>,
    request: Request,
    next: Next,
) -> impl IntoResponse {
    let token_hash = state
        .dashboard_auth
        .read()
        .await
        .as_ref()
        .map(|value| value.token_hash.clone());

    let auth_header = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer ").map(str::to_owned));

    let query_token = request.uri().query().and_then(|query| {
        query
            .split('&')
            .find_map(|part| part.strip_prefix("token=").map(str::to_owned))
    });

    match (auth_header.or(query_token), token_hash) {
        (Some(token), Some(hash)) if verify_dashboard_token(&token, &hash) => {
            next.run(request).await
        }
        (Some(_), None) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "dashboard auth not configured" })),
        )
            .into_response(),
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "unauthorized" })),
        )
            .into_response(),
    }
}
