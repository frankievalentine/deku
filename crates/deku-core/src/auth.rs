use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const TOKEN_PREFIX: &str = "dku_";
const TOKEN_ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const TOKEN_BODY_LEN: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardTokenState {
    pub token_hash: String,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Error)]
pub enum DashboardTokenError {
    #[error("dashboard token hashing failed: {0}")]
    Hash(String),
}

pub fn generate_dashboard_token() -> String {
    let mut rng = rand::thread_rng();
    let mut token = String::with_capacity(TOKEN_PREFIX.len() + TOKEN_BODY_LEN);
    token.push_str(TOKEN_PREFIX);
    for _ in 0..TOKEN_BODY_LEN {
        let idx = rng.gen_range(0..TOKEN_ALPHABET.len());
        token.push(char::from(TOKEN_ALPHABET[idx]));
    }
    token
}

pub fn hash_dashboard_token(token: &str) -> Result<String, DashboardTokenError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(token.as_bytes(), &salt)
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
    };
    Ok((token, state))
}

#[cfg(test)]
mod tests {
    use super::{issue_dashboard_token, verify_dashboard_token};

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
}
