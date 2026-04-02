// JWT auth middleware for the TCP endpoint.
// Token is generated on `deku setup` and stored in ~/.deku/config.toml.

use axum::{extract::Request, middleware::Next, response::IntoResponse};

#[allow(dead_code)]
pub async fn require_auth(request: Request, next: Next) -> impl IntoResponse {
    // Milestone 1: extract and validate JWT from Authorization header
    next.run(request).await
}
