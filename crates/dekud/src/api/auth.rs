use axum::{extract::Request, http::StatusCode, middleware::Next, response::IntoResponse, Json};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use super::SharedState;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub iat: i64,
}

/// Generate a signed JWT for CLI use from the daemon's secret.
pub fn generate_token(secret: &str) -> Result<String, jsonwebtoken::errors::Error> {
    let claims = Claims {
        sub: "deku-cli".to_string(),
        iat: chrono::Utc::now().timestamp(),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

fn validate_token(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let mut validation = Validation::default();
    // No expiry — this is a long-lived API token for a self-hosted daemon.
    validation.required_spec_claims = HashSet::new();
    validation.validate_exp = false;
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|d| d.claims)
}

/// Axum middleware that validates the Bearer JWT on TCP requests.
/// Apply only to the TCP router — the Unix socket router is trusted (local).
pub async fn require_auth(
    axum::extract::State(state): axum::extract::State<SharedState>,
    request: Request,
    next: Next,
) -> impl IntoResponse {
    let secret = match &state.config.auth_secret {
        Some(s) => s.clone(),
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "auth not configured" })),
            )
                .into_response();
        }
    };

    let auth_header = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer ").map(str::to_owned));

    let query_token = request.uri().query().and_then(|query| {
        query
            .split('&')
            .find_map(|part| part.strip_prefix("token=").map(str::to_owned))
    });

    match auth_header.or(query_token) {
        Some(token) if validate_token(&token, &secret).is_ok() => next.run(request).await,
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "unauthorized" })),
        )
            .into_response(),
    }
}
