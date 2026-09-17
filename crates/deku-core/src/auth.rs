use argon2::{
    password_hash::{phc::PasswordHash, PasswordHasher, PasswordVerifier},
    Argon2,
};
use chrono::{DateTime, TimeDelta, Utc};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const TOKEN_PREFIX: &str = "dku_";
const TOKEN_ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const TOKEN_BODY_LEN: usize = 20;
/// Lifetime applied to a dashboard token. Tokens written before this field
/// existed are treated as expiring this long after their creation time.
pub const TOKEN_TTL_DAYS: i64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardTokenState {
    pub token_hash: String,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotated_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

impl DashboardTokenState {
    /// Whether this token is at or past its expiry. A state without an explicit
    /// `expires_at` (written by an older release) expires `TOKEN_TTL_DAYS` after
    /// its creation time.
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        match self.expires_at {
            Some(expiry) => now >= expiry,
            None => now >= self.created_at + TimeDelta::days(TOKEN_TTL_DAYS),
        }
    }
}

#[derive(Debug, Error)]
pub enum DashboardTokenError {
    #[error("dashboard token hashing failed: {0}")]
    Hash(String),
}

pub fn generate_dashboard_token() -> String {
    let mut rng = rand::rng();
    let mut token = String::with_capacity(TOKEN_PREFIX.len() + TOKEN_BODY_LEN);
    token.push_str(TOKEN_PREFIX);
    for _ in 0..TOKEN_BODY_LEN {
        let idx = rng.random_range(0..TOKEN_ALPHABET.len());
        token.push(char::from(TOKEN_ALPHABET[idx]));
    }
    token
}

pub fn hash_dashboard_token(token: &str) -> Result<String, DashboardTokenError> {
    Argon2::default()
        .hash_password(token.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|err| DashboardTokenError::Hash(err.to_string()))
}

pub fn verify_dashboard_token(token: &str, hash: &str) -> bool {
    let parsed_hash = match PasswordHash::new(hash) {
        Ok(parsed) => parsed,
        Err(_) => return false,
    };

    Argon2::default()
        .verify_password(token.as_bytes(), &parsed_hash)
        .is_ok()
}

pub fn issue_dashboard_token(
    now: DateTime<Utc>,
    previous: Option<&DashboardTokenState>,
) -> Result<(String, DashboardTokenState), DashboardTokenError> {
    let token = generate_dashboard_token();
    let token_hash = hash_dashboard_token(&token)?;
    let state = DashboardTokenState {
        token_hash,
        created_at: previous.map_or(now, |value| value.created_at),
        rotated_at: previous.map(|_| now),
        expires_at: Some(now + TimeDelta::days(TOKEN_TTL_DAYS)),
    };
    Ok((token, state))
}

#[cfg(test)]
mod tests {
    use super::{
        hash_dashboard_token, issue_dashboard_token, verify_dashboard_token, DashboardTokenState,
        TOKEN_TTL_DAYS,
    };

    const LEGACY_TOKEN: &str = "dku_LEGACYCOMPAT000000";

    /// PHC string produced by argon2 0.5.3 before the dependency upgrade.
    const LEGACY_ARGON2_05_HASH: &str = concat!(
        "$argon2id$v=19$m=19456,t=2,p=1$ZGVrdS1sZWdhY3ktc2FsdA",
        "$gggQiL5iX4oC8kmpRjhgY8sZRNfqZ8n83Kzh9Wnyafk"
    );

    #[test]
    fn issued_token_verifies_against_stored_hash() {
        let now = chrono::Utc::now();
        let (token, state) = issue_dashboard_token(now, None).expect("issue token");
        assert!(verify_dashboard_token(&token, &state.token_hash));
        assert!(!verify_dashboard_token(
            "dku_INVALIDTOKEN00000",
            &state.token_hash
        ));
        assert!(state.rotated_at.is_none());
    }

    #[test]
    fn rotated_token_tracks_original_creation_time() {
        let now = chrono::Utc::now();
        let (_, state) = issue_dashboard_token(now, None).expect("issue token");
        let later = now + chrono::TimeDelta::seconds(30);
        let (_, rotated) = issue_dashboard_token(later, Some(&state)).expect("rotate token");
        assert_eq!(rotated.created_at, state.created_at);
        assert_eq!(rotated.rotated_at, Some(later));
    }

    #[test]
    fn hash_written_by_argon2_0_5_still_verifies_after_upgrade() {
        assert!(verify_dashboard_token(LEGACY_TOKEN, LEGACY_ARGON2_05_HASH));
        assert!(!verify_dashboard_token(
            "dku_WRONGTOKEN0000000",
            LEGACY_ARGON2_05_HASH
        ));
        assert!(!verify_dashboard_token(LEGACY_TOKEN, "not-a-phc-hash"));
    }

    #[test]
    fn hashing_one_token_twice_uses_distinct_random_salts() {
        let first = hash_dashboard_token(LEGACY_TOKEN).expect("hash token");
        let second = hash_dashboard_token(LEGACY_TOKEN).expect("hash token");

        let salt_of = |hash: &str| hash.split('$').nth(4).map(str::to_owned);
        assert_ne!(salt_of(&first), salt_of(&second));
        assert!(verify_dashboard_token(LEGACY_TOKEN, &first));
        assert!(verify_dashboard_token(LEGACY_TOKEN, &second));
    }

    #[test]
    fn issued_token_carries_a_bounded_expiry() {
        let now = chrono::Utc::now();
        let (_, state) = issue_dashboard_token(now, None).expect("issue token");
        assert_eq!(
            state.expires_at,
            Some(now + chrono::TimeDelta::days(TOKEN_TTL_DAYS))
        );
        assert!(!state.is_expired(now));
        assert!(!state.is_expired(now + chrono::TimeDelta::days(TOKEN_TTL_DAYS - 1)));
        assert!(state.is_expired(now + chrono::TimeDelta::days(TOKEN_TTL_DAYS)));
    }

    #[test]
    fn legacy_token_without_expiry_uses_creation_time_plus_ttl() {
        let now = chrono::Utc::now();
        let fresh = DashboardTokenState {
            token_hash: "hash".to_string(),
            created_at: now,
            rotated_at: None,
            expires_at: None,
        };
        assert!(!fresh.is_expired(now));

        let stale = DashboardTokenState {
            token_hash: "hash".to_string(),
            created_at: now - chrono::TimeDelta::days(TOKEN_TTL_DAYS + 1),
            rotated_at: None,
            expires_at: None,
        };
        assert!(stale.is_expired(now));
    }
}
