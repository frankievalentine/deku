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

/// The address a request is rate-limited by.
///
/// A request through the dashboard's Angie vhost arrives from loopback, which
/// the limiter exempts, so the client address Angie forwards in `X-Real-IP` is
/// used instead. The header is only trusted from loopback: a remote client
/// connecting directly cannot use it to pose as someone else.
fn client_ip(peer: IpAddr, real_ip: Option<&str>) -> IpAddr {
    if !peer.is_loopback() {
        return peer;
    }
    real_ip
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(peer)
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

/// The deploy token carried in a `token=` query parameter, if any.
///
/// Only app-scoped deploy tokens are read from the query, for provider webhooks
/// that cannot set headers. The dashboard token grants the whole API, so it must
/// arrive in the Authorization header: URLs end up in browser history and proxy
/// access logs.
fn query_deploy_token(query: Option<&str>) -> Option<String> {
    query?
        .split('&')
        .find_map(|part| part.strip_prefix("token="))
        .filter(|token| deploy_token::looks_like_deploy_token(token))
        .map(str::to_owned)
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
        let real_ip = request
            .headers()
            .get("x-real-ip")
            .and_then(|value| value.to_str().ok());
        if !rate_limit_allow(client_ip(peer.ip(), real_ip)) {
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

    let presented = auth_header.or_else(|| query_deploy_token(request.uri().query()));

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
    use super::{client_ip, query_deploy_token, Bucket, RATE_LIMIT_CAPACITY};
    use crate::services::deploy_token::DEPLOY_TOKEN_PREFIX;
    use std::net::IpAddr;
    use std::time::Instant;

    #[test]
    fn proxied_requests_are_limited_by_the_forwarded_client() {
        let loopback: IpAddr = "127.0.0.1".parse().expect("ip");
        let remote: IpAddr = "203.0.113.7".parse().expect("ip");
        assert_eq!(client_ip(loopback, Some("203.0.113.7")), remote);
        assert_eq!(client_ip(loopback, None), loopback);
        assert_eq!(client_ip(loopback, Some("not-an-ip")), loopback);
    }

    #[test]
    fn a_direct_client_cannot_choose_its_address() {
        let remote: IpAddr = "203.0.113.7".parse().expect("ip");
        assert_eq!(client_ip(remote, Some("127.0.0.1")), remote);
    }

    #[test]
    fn query_accepts_a_deploy_token() {
        let token = format!("{DEPLOY_TOKEN_PREFIX}abc123");
        let query = format!("ref=main&token={token}");
        assert_eq!(query_deploy_token(Some(&query)), Some(token));
    }

    #[test]
    fn query_ignores_a_dashboard_token() {
        assert_eq!(query_deploy_token(Some("token=dku_dashboard_secret")), None);
        assert_eq!(query_deploy_token(Some("search=error")), None);
        assert_eq!(query_deploy_token(None), None);
    }

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
