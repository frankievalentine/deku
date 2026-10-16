//! Dashboard browser sessions.
//!
//! The dashboard token grants the whole API, so a browser should not keep it
//! where page scripts can read it. The browser presents the token once, as a
//! bearer header, to `POST /api/dashboard/session`; the daemon answers with an
//! HttpOnly, SameSite=Strict cookie naming a session, and an XSS bug in the
//! dashboard can no longer carry the token away.
//!
//! A cookie is sent by the browser on its own, so a state-changing request that
//! authenticates with one must also carry [`CSRF_HEADER`], which a cross-site
//! form cannot set, and an `Origin` that matches the host.

use std::net::IpAddr;

use anyhow::Result;
use axum::http::{HeaderMap, Method};
use chrono::{DateTime, TimeDelta, Utc};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

pub const COOKIE: &str = "deku_session";

/// The header a cookie-authenticated request must send to change anything.
pub const CSRF_HEADER: &str = "x-deku-csrf";

/// How long a session lasts when the token itself has no expiry.
const DEFAULT_LIFETIME: TimeDelta = TimeDelta::days(30);

/// How a request was authenticated, recorded for the handlers that care.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthenticatedBy {
    /// The dashboard token, in the Authorization header.
    Bearer,
    /// A session cookie; holds the session id.
    Session(String),
}

/// A stored session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
}

fn hash_id(id: &str) -> String {
    hex::encode(Sha256::digest(id.as_bytes()))
}

/// When a session issued now under a token with `token_expires` should end.
pub fn session_expiry(token_expires: Option<DateTime<Utc>>, now: DateTime<Utc>) -> DateTime<Utc> {
    token_expires.unwrap_or(now + DEFAULT_LIFETIME)
}

/// Start a session for the token whose hash is `token_hash`; returns its id.
pub async fn create(
    pool: &SqlitePool,
    token_hash: &str,
    expires_at: DateTime<Utc>,
) -> Result<String> {
    use rand::RngExt;

    let now = Utc::now();
    sqlx::query("DELETE FROM dashboard_sessions WHERE expires_at <= ?1")
        .bind(now)
        .execute(pool)
        .await?;

    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    let id = hex::encode(bytes);
    sqlx::query(
        "INSERT INTO dashboard_sessions (id_hash, token_hash, created_at, expires_at)
         VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(hash_id(&id))
    .bind(token_hash)
    .bind(now)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(id)
}

/// The session `id` names, if there is one.
pub async fn lookup(pool: &SqlitePool, id: &str) -> Result<Option<Session>> {
    use sqlx::Row;

    let row =
        sqlx::query("SELECT token_hash, expires_at FROM dashboard_sessions WHERE id_hash = ?1")
            .bind(hash_id(id))
            .fetch_optional(pool)
            .await?;
    Ok(match row {
        Some(row) => Some(Session {
            token_hash: row.try_get("token_hash")?,
            expires_at: row.try_get("expires_at")?,
        }),
        None => None,
    })
}

/// Whether `session` is still good for the current token.
pub fn is_valid(session: &Session, current_token_hash: &str, now: DateTime<Utc>) -> bool {
    session.expires_at > now && session.token_hash == current_token_hash
}

/// End one session.
pub async fn delete(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM dashboard_sessions WHERE id_hash = ?1")
        .bind(hash_id(id))
        .execute(pool)
        .await?;
    Ok(())
}

/// End every session, as rotating the token does.
pub async fn delete_all(pool: &SqlitePool) -> Result<()> {
    sqlx::query("DELETE FROM dashboard_sessions")
        .execute(pool)
        .await?;
    Ok(())
}

/// The session id the request's cookie carries.
pub fn cookie_value(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(axum::http::header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .find_map(|pair| {
            let (name, value) = pair.trim().split_once('=')?;
            (name == COOKIE && !value.is_empty()).then(|| value.to_string())
        })
}

/// Whether the browser reached the daemon over HTTPS.
///
/// The daemon itself speaks plain HTTP, so HTTPS only ever arrives through the
/// dashboard's Angie vhost, which says so in `X-Forwarded-Proto`. The header is
/// believed only from loopback, where Angie connects from.
pub fn request_is_https(peer: Option<IpAddr>, headers: &HeaderMap) -> bool {
    peer.is_some_and(|peer| peer.is_loopback())
        && headers
            .get("x-forwarded-proto")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.eq_ignore_ascii_case("https"))
}

/// The `Set-Cookie` value that starts a session.
///
/// `Secure` is set only over HTTPS: over an SSH tunnel the dashboard is plain
/// http://127.0.0.1, where a Secure cookie would never be sent back.
pub fn set_cookie(id: &str, expires_at: DateTime<Utc>, secure: bool, now: DateTime<Utc>) -> String {
    let max_age = (expires_at - now).num_seconds().max(0);
    format!(
        "{COOKIE}={id}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age}{}",
        if secure { "; Secure" } else { "" }
    )
}

/// The `Set-Cookie` value that removes the session cookie.
pub fn clear_cookie(secure: bool) -> String {
    format!(
        "{COOKIE}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0{}",
        if secure { "; Secure" } else { "" }
    )
}

/// Whether a cookie-authenticated request may go ahead.
///
/// Reads are always allowed. A write needs [`CSRF_HEADER`], which a page on
/// another site cannot add without a CORS preflight the daemon never grants,
/// and an `Origin`, when the browser sends one, naming this host.
pub fn csrf_allows(method: &Method, headers: &HeaderMap) -> bool {
    if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return true;
    }
    if headers.get(CSRF_HEADER).is_none() {
        return false;
    }
    let Some(origin) = headers
        .get(axum::http::header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    else {
        return true;
    };
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    origin
        .split_once("://")
        .is_some_and(|(_, authority)| authority.eq_ignore_ascii_case(host))
}

#[cfg(test)]
mod tests {
    use super::{
        clear_cookie, cookie_value, create, csrf_allows, delete_all, is_valid, lookup,
        request_is_https, session_expiry, set_cookie, COOKIE, CSRF_HEADER,
    };
    use axum::http::{HeaderMap, HeaderValue, Method};
    use chrono::{TimeDelta, Utc};

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(*name, HeaderValue::from_str(value).expect("header"));
        }
        map
    }

    async fn pool() -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        crate::db::migrate(&pool).await.expect("migrate");
        pool
    }

    #[tokio::test]
    async fn a_session_is_found_by_its_id_and_ends_with_its_token() {
        let pool = pool().await;
        let now = Utc::now();
        let id = create(&pool, "token-a", now + TimeDelta::days(1))
            .await
            .expect("create");
        let session = lookup(&pool, &id).await.expect("lookup").expect("found");
        assert!(is_valid(&session, "token-a", now));
        assert!(
            !is_valid(&session, "token-b", now),
            "a rotated token ends the session"
        );
        assert!(
            !is_valid(&session, "token-a", now + TimeDelta::days(2)),
            "and so does expiry"
        );

        let stored: String = sqlx::query_scalar("SELECT id_hash FROM dashboard_sessions")
            .fetch_one(&pool)
            .await
            .expect("row");
        assert_ne!(stored, id, "only a hash of the id is stored");

        delete_all(&pool).await.expect("delete");
        assert!(lookup(&pool, &id).await.expect("lookup").is_none());
    }

    #[test]
    fn the_cookie_is_http_only_strict_and_secure_only_over_https() {
        let now = Utc::now();
        let cookie = set_cookie("abc", now + TimeDelta::hours(1), false, now);
        assert_eq!(
            cookie,
            format!("{COOKIE}=abc; Path=/; HttpOnly; SameSite=Strict; Max-Age=3600")
        );
        assert!(set_cookie("abc", now + TimeDelta::hours(1), true, now).ends_with("; Secure"));
        assert!(clear_cookie(false).contains("Max-Age=0"));
        assert_eq!(session_expiry(None, now), now + TimeDelta::days(30));
    }

    #[test]
    fn the_session_cookie_is_read_among_others() {
        let map = headers(&[("cookie", "theme=dark; deku_session=s3cr3t; other=1")]);
        assert_eq!(cookie_value(&map).as_deref(), Some("s3cr3t"));
        assert_eq!(cookie_value(&headers(&[("cookie", "deku_session=")])), None);
        assert_eq!(cookie_value(&HeaderMap::new()), None);
    }

    #[test]
    fn https_is_believed_only_from_the_local_proxy() {
        let proto = headers(&[("x-forwarded-proto", "https")]);
        assert!(request_is_https(
            Some("127.0.0.1".parse().expect("ip")),
            &proto
        ));
        assert!(!request_is_https(
            Some("203.0.113.7".parse().expect("ip")),
            &proto
        ));
        assert!(!request_is_https(
            Some("127.0.0.1".parse().expect("ip")),
            &HeaderMap::new()
        ));
    }

    #[test]
    fn writes_need_the_csrf_header_and_a_matching_origin() {
        assert!(
            csrf_allows(&Method::GET, &HeaderMap::new()),
            "reads are always allowed"
        );
        assert!(
            !csrf_allows(&Method::POST, &HeaderMap::new()),
            "a cross-site form sends no header"
        );

        let same = headers(&[
            (CSRF_HEADER, "1"),
            ("host", "127.0.0.1:2810"),
            ("origin", "http://127.0.0.1:2810"),
        ]);
        assert!(csrf_allows(&Method::POST, &same));

        let cross = headers(&[
            (CSRF_HEADER, "1"),
            ("host", "127.0.0.1:2810"),
            ("origin", "https://evil.example"),
        ]);
        assert!(!csrf_allows(&Method::DELETE, &cross));

        let no_origin = headers(&[(CSRF_HEADER, "1"), ("host", "dash.example.com")]);
        assert!(
            csrf_allows(&Method::PUT, &no_origin),
            "same-origin requests may omit Origin"
        );
    }
}
