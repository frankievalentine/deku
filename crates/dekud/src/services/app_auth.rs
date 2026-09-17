//! Per-app HTTP authentication (proxy-layer basic auth and forward auth).

use anyhow::{anyhow, Result};
use sqlx::SqlitePool;

use crate::db::queries;

/// Minimum length enforced for a basic-auth password.
pub const MIN_PASSWORD_LEN: usize = 8;

/// Hash a password for an Angie htpasswd file.
///
/// Angie (NGINX-compatible) `auth_basic_user_file` supports crypt() hashes and
/// the Apache `$apr1$` variant, not bcrypt, so use SHA-512 crypt (`$6$`).
pub fn hash_password(password: &str) -> Result<String> {
    pwhash::sha512_crypt::hash(password).map_err(|error| anyhow!("hashing password: {error}"))
}

fn validate_username(username: &str) -> Result<()> {
    let trimmed = username.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("username must not be empty"));
    }
    if trimmed.contains(':') || trimmed.contains('\n') || trimmed.contains('\r') {
        return Err(anyhow!("username must not contain ':' or newlines"));
    }
    Ok(())
}

fn validate_forward_url(url: &str) -> Result<()> {
    let trimmed = url.trim();
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err(anyhow!(
            "forward auth URL must start with http:// or https://"
        ));
    }
    Ok(())
}

/// Enable basic auth for an app, replacing any existing auth.
pub async fn enable_basic(
    pool: &SqlitePool,
    app_name: &str,
    username: &str,
    password: &str,
) -> Result<()> {
    validate_username(username)?;
    if password.len() < MIN_PASSWORD_LEN {
        return Err(anyhow!(
            "password must be at least {MIN_PASSWORD_LEN} characters"
        ));
    }
    let app = queries::get_app(pool, app_name).await?;
    let hash = hash_password(password)?;
    queries::upsert_app_auth_basic(pool, &app.id, username.trim(), &hash).await?;
    Ok(())
}

/// Enable forward auth (an external `auth_request` endpoint) for an app.
pub async fn enable_forward(pool: &SqlitePool, app_name: &str, forward_url: &str) -> Result<()> {
    validate_forward_url(forward_url)?;
    let app = queries::get_app(pool, app_name).await?;
    queries::upsert_app_auth_forward(pool, &app.id, forward_url.trim()).await?;
    Ok(())
}

/// Disable auth for an app.
pub async fn disable(pool: &SqlitePool, app_name: &str) -> Result<()> {
    let app = queries::get_app(pool, app_name).await?;
    queries::delete_app_auth(pool, &app.id).await?;
    Ok(())
}

/// Current auth record for an app, if any.
pub async fn status(pool: &SqlitePool, app_name: &str) -> Result<Option<queries::AppAuthRecord>> {
    let app = queries::get_app(pool, app_name).await?;
    Ok(queries::get_app_auth(pool, &app.id).await?)
}

#[cfg(test)]
mod tests {
    use super::{hash_password, validate_forward_url, validate_username};

    #[test]
    fn sha512_crypt_hash_is_supported_by_angie() {
        let hash = hash_password("correct horse battery").expect("hash");
        assert!(
            hash.starts_with("$6$"),
            "expected SHA-512 crypt, got {hash}"
        );
        assert!(pwhash::sha512_crypt::verify("correct horse battery", &hash));
        assert!(!pwhash::sha512_crypt::verify("wrong", &hash));
    }

    #[test]
    fn username_and_forward_url_validation() {
        assert!(validate_username("").is_err());
        assert!(validate_username("a:b").is_err());
        assert!(validate_username("alice").is_ok());
        assert!(validate_forward_url("ftp://x").is_err());
        assert!(validate_forward_url("https://auth.example/verify").is_ok());
    }
}
