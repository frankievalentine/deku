//! Per-app deploy tokens for CI and provider webhooks.
//!
//! A deploy token authorizes only the deploy routes of one app, so a leaked
//! token cannot touch anything else. Tokens are high-entropy random strings, so
//! a fast SHA-256 digest is enough to store them; unlike the dashboard token
//! (a password-equivalent secret verified with Argon2id), there is no
//! password-guessing surface to slow down.

use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use deku_core::error::Result;

use crate::db::queries::{self, DeployToken};

/// Distinguishes deploy tokens from dashboard tokens (`dku_`).
pub const DEPLOY_TOKEN_PREFIX: &str = "dkt_";

/// Crockford-style alphabet, matching the dashboard token: no I, L, O, or U, so
/// a token copied by hand cannot be misread.
const TOKEN_ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const TOKEN_BODY_LEN: usize = 32;
/// Characters of the token kept for display after creation.
const DISPLAY_PREFIX_LEN: usize = 12;

pub fn generate_token() -> String {
    use rand::RngExt;

    let mut rng = rand::rng();
    let mut token = String::with_capacity(DEPLOY_TOKEN_PREFIX.len() + TOKEN_BODY_LEN);
    token.push_str(DEPLOY_TOKEN_PREFIX);
    for _ in 0..TOKEN_BODY_LEN {
        let idx = rng.random_range(0..TOKEN_ALPHABET.len());
        token.push(char::from(TOKEN_ALPHABET[idx]));
    }
    token
}

pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

pub fn display_prefix(token: &str) -> String {
    token.chars().take(DISPLAY_PREFIX_LEN).collect()
}

pub fn looks_like_deploy_token(token: &str) -> bool {
    token.starts_with(DEPLOY_TOKEN_PREFIX)
}

/// Create a token for `app_id` and return it with the plaintext, which is only
/// ever available at this point.
pub async fn create(pool: &SqlitePool, app_id: &str, name: &str) -> Result<(DeployToken, String)> {
    let token = generate_token();
    let record = queries::create_deploy_token(
        pool,
        app_id,
        name,
        &hash_token(&token),
        &display_prefix(&token),
    )
    .await?;
    Ok((record, token))
}

/// Resolve a presented token to the app it may deploy, recording the use.
pub async fn authorize(pool: &SqlitePool, token: &str, app_id: &str) -> Result<bool> {
    let Some(record) = queries::get_deploy_token_by_hash(pool, &hash_token(token)).await? else {
        return Ok(false);
    };

    if record.app_id != app_id {
        return Ok(false);
    }

    queries::touch_deploy_token(pool, &record.id).await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::{
        display_prefix, generate_token, hash_token, looks_like_deploy_token, DEPLOY_TOKEN_PREFIX,
        DISPLAY_PREFIX_LEN, TOKEN_BODY_LEN,
    };

    #[test]
    fn generated_tokens_are_prefixed_and_full_length() {
        let token = generate_token();
        assert!(token.starts_with(DEPLOY_TOKEN_PREFIX));
        assert_eq!(token.len(), DEPLOY_TOKEN_PREFIX.len() + TOKEN_BODY_LEN);
        assert!(looks_like_deploy_token(&token));
        assert!(!looks_like_deploy_token("dku_ABCDEFGHJKMNPQRSTVWXYZ01"));
    }

    #[test]
    fn generated_tokens_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..64 {
            assert!(seen.insert(generate_token()));
        }
    }

    #[test]
    fn hash_is_stable_and_hex_encoded() {
        let token = generate_token();
        let hash = hash_token(&token);
        assert_eq!(hash, hash_token(&token));
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(hash, hash_token("dkt_OTHERTOKEN000000000000000000000"));
    }

    #[test]
    fn display_prefix_is_a_strict_prefix_of_the_token() {
        let token = "dkt_ABCDEFGHJKMNPQRSTVWXYZ01234567";
        let prefix = display_prefix(token);
        assert_eq!(prefix, "dkt_ABCDEFGH");
        assert_eq!(prefix.len(), DISPLAY_PREFIX_LEN);
        assert!(token.starts_with(&prefix));
        assert!(prefix.len() < token.len());
    }
}
