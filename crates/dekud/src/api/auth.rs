use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use axum::{
    extract::ConnectInfo,
    http::{header, StatusCode},
    middleware::Next,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use deku_core::auth::verify_dashboard_token;
use serde_json::json;
use tokio::sync::Semaphore;
use tracing::{info, warn};

use super::SharedState;
use crate::db::queries;
use crate::services::deploy_token;

/// Bound the number of concurrent Argon2id verifications so a burst of
/// unauthenticated requests cannot exhaust CPU or the blocking thread pool.
fn verify_permits() -> &'static Semaphore {
    static PERMITS: OnceLock<Semaphore> = OnceLock::new();
    PERMITS.get_or_init(|| Semaphore::new(2))
}

/// Per-client-IP token bucket. It bounds how often one remote address can make
/// the daemon run an Argon2 verification, without affecting loopback clients.
const RATE_LIMIT_CAPACITY: f64 = 120.0;
const RATE_LIMIT_REFILL_PER_SECOND: f64 = 2.0;
const RATE_LIMIT_MAX_ENTRIES: usize = 10_000;
const RATE_LIMIT_STALE_SECONDS: f64 = 300.0;

struct Bucket {
    tokens: f64,
    last: Instant,
}

impl Bucket {
    fn try_take(&mut self, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens =
            (self.tokens + elapsed * RATE_LIMIT_REFILL_PER_SECOND).min(RATE_LIMIT_CAPACITY);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

fn rate_limit_state() -> &'static Mutex<HashMap<IpAddr, Bucket>> {
    static STATE: OnceLock<Mutex<HashMap<IpAddr, Bucket>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn rate_limit_allow(ip: IpAddr) -> bool {
    if ip.is_loopback() {
        return true;
    }

    let now = Instant::now();
    let mut guard = match rate_limit_state().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    if guard.len() > RATE_LIMIT_MAX_ENTRIES {
        guard.retain(|_, bucket| {
            now.saturating_duration_since(bucket.last).as_secs_f64() < RATE_LIMIT_STALE_SECONDS
        });
    }

    guard
        .entry(ip)
        .or_insert(Bucket {
            tokens: RATE_LIMIT_CAPACITY,
            last: now,
        })
        .try_take(now)
}

async fn verify_token(token: String, hash: String) -> bool {
    let Ok(_permit) = verify_permits().acquire().await else {
        return false;
    };

    tokio::task::spawn_blocking(move || verify_dashboard_token(&token, &hash))
        .await
        .unwrap_or(false)
}

fn unauthorized() -> axum::response::Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": "unauthorized" })),
    )
        .into_response()
}

fn forbidden(message: &str) -> axum::response::Response {
    (StatusCode::FORBIDDEN, Json(json!({ "error": message }))).into_response()
}

/// The app a deploy token is allowed to act on, for this method and path.
///
/// Deploy tokens are deliberately narrow: they cover the two routes CI needs to
/// ship a release, and nothing else.
fn deploy_route_app<'a>(method: &axum::http::Method, path: &'a str) -> Option<&'a str> {
    if method != axum::http::Method::POST {
        return None;
    }
    let rest = path.strip_prefix("/api/apps/")?;
    let (app, tail) = rest.split_once('/')?;
    if app.is_empty() {
        return None;
    }
    match tail {
        "deploy" | "deploy/archive" => Some(app),
        _ => None,
    }
}

enum DeployTokenOutcome {
    Allow,
    Deny,
}

/// Validate an app-scoped deploy token against the requested route.
async fn authorize_deploy_token(
    state: &SharedState,
    token: &str,
    method: &axum::http::Method,
    path: &str,
) -> Result<DeployTokenOutcome, String> {
    let Some(app_name) = deploy_route_app(method, path) else {
        return Err("deploy token is only valid for deploy routes".to_string());
    };

    // Resolve the target app; an unknown app is reported as a rejected token so
    // this cannot be used to probe which apps exist.
    let app_id = match queries::get_app(&state.pool, app_name).await {
        Ok(app) => app.id,
        Err(_) => return Ok(DeployTokenOutcome::Deny),
    };

    match crate::services::deploy_token::authorize(&state.pool, token, &app_id).await {
        Ok(true) => {
            info!(app = %app_name, "deploy token accepted");
            Ok(DeployTokenOutcome::Allow)
        }
        Ok(false) => Ok(DeployTokenOutcome::Deny),
        Err(error) => Err(error.to_string()),
    }
}

/// Axum middleware that validates the dashboard bearer token on TCP requests.
/// Apply only to the TCP router — the Unix socket router is trusted local access.
pub async fn require_auth(
    axum::extract::State(state): axum::extract::State<SharedState>,
    request: axum::extract::Request,
    next: Next,
) -> impl IntoResponse {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();

    if let Some(ConnectInfo(peer)) = request.extensions().get::<ConnectInfo<SocketAddr>>() {
        if !rate_limit_allow(peer.ip()) {
            warn!(%method, %path, "dashboard auth rate limit exceeded");
            return (
                StatusCode::TOO_MANY_REQUESTS,
                [(header::RETRY_AFTER, "30")],
                Json(serde_json::json!({ "error": "too many requests" })),
            )
                .into_response();
        }
    }

    let token_state = state.dashboard_auth.read().await.clone();

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

    let presented = auth_header.or(query_token);

    // App-scoped deploy tokens for CI and provider webhooks.
    if let Some(token) = presented.as_deref() {
        if deploy_token::looks_like_deploy_token(token) {
            return match authorize_deploy_token(&state, token, &method, &path).await {
                Ok(DeployTokenOutcome::Allow) => next.run(request).await,
                Ok(DeployTokenOutcome::Deny) => {
                    warn!(%method, %path, "deploy token rejected");
                    unauthorized()
                }
                Err(reason) => {
                    warn!(%method, %path, "deploy token rejected: {reason}");
                    forbidden(&reason)
                }
            };
        }
    }

    match (presented, token_state) {
        (Some(token), Some(auth_state)) => {
            if auth_state.is_expired(Utc::now()) {
                warn!(%method, %path, "dashboard auth rejected expired token");
                return unauthorized();
            }
            if verify_token(token, auth_state.token_hash).await {
                next.run(request).await
            } else {
                warn!(%method, %path, "dashboard auth rejected request with invalid token");
                unauthorized()
            }
        }
        (Some(_), None) => {
            warn!(%method, %path, "dashboard auth is not configured");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "dashboard auth not configured" })),
            )
                .into_response()
        }
        (None, _) => {
            warn!(%method, %path, "dashboard auth rejected request without token");
            unauthorized()
        }
    }
}

pub fn log_dashboard_session_verified() {
    info!("dashboard token accepted for browser session");
}

#[cfg(test)]
mod tests {
    use super::{Bucket, RATE_LIMIT_CAPACITY};
    use std::time::Instant;

    #[test]
    fn rate_limit_bucket_allows_burst_then_denies() {
        let start = Instant::now();
        let mut bucket = Bucket {
            tokens: RATE_LIMIT_CAPACITY,
            last: start,
        };

        for _ in 0..(RATE_LIMIT_CAPACITY as usize) {
            assert!(bucket.try_take(start));
        }
        assert!(!bucket.try_take(start));

        let later = start + std::time::Duration::from_secs(1);
        assert!(bucket.try_take(later));
    }
}
