//! Config var encryption at rest.
//!
//! Config var values are stored in the state database. A database copy is easy to
//! take and easy to lose, and the values are exactly the things operators treat
//! as secret: database URLs with passwords, API keys, tokens. When an
//! encryption-at-rest key is configured, values are sealed on write and opened on
//! read with the same envelope used for service backups.
//!
//! Encryption is transparent to callers: [`get_config_vars`] always returns
//! plaintext, whether the row was written encrypted or not. That means the
//! function every injection path already calls is also the safe one, and a new
//! call site cannot forget to decrypt.
//!
//! With no key configured values are stored in the clear, exactly as before, so
//! existing installs keep working; `deku doctor` reports which mode is active.

use anyhow::{anyhow, Result};
use deku_core::types::ConfigVar;
use serde::Serialize;
use sqlx::SqlitePool;

use crate::config::DekuConfig;
use crate::crypto::ENVELOPE_MAGIC_SECRET;
use crate::db::queries;

/// A config var prepared for display, where a value may be unreadable.
#[derive(Debug, Serialize)]
pub struct ConfigVarView {
    pub app_id: String,
    pub key: String,
    /// Plaintext when readable; empty when the value cannot be decrypted.
    pub value: String,
    pub is_global: bool,
    /// Whether the stored row is ciphertext.
    pub encrypted: bool,
    /// Whether the value could not be decrypted, and why.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Encrypt a value for storage, or store it as-is when no key is configured.
pub fn encrypt_value(cfg: &DekuConfig, value: &str) -> Result<String> {
    match cfg.at_rest_cipher()? {
        Some(cipher) => {
            let sealed = cipher.seal(ENVELOPE_MAGIC_SECRET, value.as_bytes())?;
            // The envelope is binary (nonce + tag); base64 keeps it a valid TEXT
            // column value and keeps the database readable by humans and tooling.
            Ok(format!(
                "{}{}",
                ENVELOPE_PREFIX,
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, sealed)
            ))
        }
        None => Ok(value.to_string()),
    }
}

/// Decrypt a stored value, passing plaintext rows through untouched.
pub fn decrypt_value(cfg: &DekuConfig, stored: &str) -> Result<String> {
    let Some(encoded) = stored.strip_prefix(ENVELOPE_PREFIX) else {
        return Ok(stored.to_string());
    };

    let cipher = cfg.at_rest_cipher()?.ok_or_else(|| {
        anyhow!(
            "value is encrypted but no key is configured; set DEKU_ENCRYPTION_KEY or [encryption]"
        )
    })?;
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
        .map_err(|error| anyhow!("encrypted config value is not valid base64: {error}"))?;
    let plaintext = cipher.open(ENVELOPE_MAGIC_SECRET, &bytes)?;
    String::from_utf8(plaintext)
        .map_err(|error| anyhow!("decrypted config value is not valid UTF-8: {error}"))
}

/// Whether a stored value carries the config var envelope.
pub fn is_encrypted_value(stored: &str) -> bool {
    stored.starts_with(ENVELOPE_PREFIX)
}

/// Store a config var, encrypting the value when a key is configured.
pub async fn set_config_var(
    pool: &SqlitePool,
    cfg: &DekuConfig,
    app_id: &str,
    key: &str,
    value: &str,
    is_global: bool,
) -> Result<()> {
    let stored = encrypt_value(cfg, value)?;
    queries::set_config_var_raw(pool, app_id, key, &stored, is_global).await?;
    Ok(())
}

/// Config vars with plaintext values, for injection into a container.
///
/// Fails rather than returning a value it could not decrypt: injecting a broken
/// secret into a running app is worse than failing the deploy.
pub async fn get_config_vars(
    pool: &SqlitePool,
    cfg: &DekuConfig,
    app_id: &str,
) -> Result<Vec<ConfigVar>> {
    let mut vars = queries::get_config_vars_raw(pool, app_id).await?;
    for var in &mut vars {
        var.value = decrypt_value(cfg, &var.value)
            .map_err(|error| anyhow!("config var '{}' could not be decrypted: {error}", var.key))?;
    }
    Ok(vars)
}

/// Config vars for one environment: app-wide values with that environment's
/// overrides applied on top.
///
/// This is what the deploy path injects. Passing `None` for the environment gives
/// the app-wide set alone.
pub async fn resolve_config_vars(
    pool: &SqlitePool,
    cfg: &DekuConfig,
    app_id: &str,
    environment_id: Option<&str>,
) -> Result<Vec<ConfigVar>> {
    let mut merged = get_config_vars(pool, cfg, app_id).await?;

    let Some(environment_id) = environment_id else {
        return Ok(merged);
    };

    let mut overrides =
        queries::get_environment_config_vars_raw(pool, app_id, environment_id).await?;
    for replacement in &mut overrides {
        replacement.value = decrypt_value(cfg, &replacement.value).map_err(|error| {
            anyhow!(
                "config var '{}' could not be decrypted: {error}",
                replacement.key
            )
        })?;

        match merged
            .iter_mut()
            .find(|existing| existing.key == replacement.key)
        {
            Some(existing) => *existing = replacement.clone(),
            None => merged.push(replacement.clone()),
        }
    }

    Ok(merged)
}

/// Config vars for display, where a single unreadable value must not hide the rest.
pub async fn list_config_vars(
    pool: &SqlitePool,
    cfg: &DekuConfig,
    app_id: &str,
) -> Result<Vec<ConfigVarView>> {
    let var = queries::get_config_vars_raw(pool, app_id).await?;
    Ok(var
        .into_iter()
        .map(|var| {
            let encrypted = is_encrypted_value(&var.value);
            match decrypt_value(cfg, &var.value) {
                Ok(value) => ConfigVarView {
                    app_id: var.app_id,
                    key: var.key,
                    value,
                    is_global: var.is_global,
                    encrypted,
                    error: None,
                },
                Err(error) => ConfigVarView {
                    app_id: var.app_id,
                    key: var.key,
                    value: String::new(),
                    is_global: var.is_global,
                    encrypted,
                    error: Some(error.to_string()),
                },
            }
        })
        .collect())
}

/// The prefix that marks a stored config var value as ciphertext.
const ENVELOPE_PREFIX: &str = "enc:v1:";

#[cfg(test)]
mod tests {
    use super::{decrypt_value, encrypt_value, is_encrypted_value, ConfigVarView, ENVELOPE_PREFIX};
    use crate::config::{DekuConfig, EncryptionConfig};

    fn cfg_with_key(hex: &str) -> DekuConfig {
        DekuConfig {
            encryption: Some(EncryptionConfig {
                key: Some(hex.to_string()),
                key_file: None,
            }),
            ..DekuConfig::default()
        }
    }

    fn key() -> String {
        "ab".repeat(32)
    }

    #[test]
    fn round_trips_a_value_when_a_key_is_configured() {
        let cfg = cfg_with_key(&key());
        let stored = encrypt_value(&cfg, "postgres://user:pw@db:5432/app").expect("encrypt");
        assert!(is_encrypted_value(&stored));
        assert!(stored.starts_with(ENVELOPE_PREFIX));
        assert!(
            !stored.contains("pw"),
            "the plaintext must not survive in the stored value"
        );
        assert_eq!(
            decrypt_value(&cfg, &stored).expect("decrypt"),
            "postgres://user:pw@db:5432/app"
        );
    }

    #[test]
    fn stores_plaintext_when_no_key_is_configured() {
        let cfg = DekuConfig::default();
        let stored = encrypt_value(&cfg, "LOG_LEVEL=info").expect("encrypt");
        assert_eq!(stored, "LOG_LEVEL=info");
        assert!(!is_encrypted_value(&stored));
        assert_eq!(
            decrypt_value(&cfg, &stored).expect("decrypt"),
            "LOG_LEVEL=info"
        );
    }

    #[test]
    fn plaintext_rows_still_read_after_a_key_is_added() {
        // Rows written before encryption was enabled must keep working.
        let cfg = cfg_with_key(&key());
        assert_eq!(
            decrypt_value(&cfg, "LEGACY=value").expect("decrypt"),
            "LEGACY=value"
        );
    }

    #[test]
    fn an_encrypted_value_needs_the_key() {
        let stored = encrypt_value(&cfg_with_key(&key()), "SECRET=1").expect("encrypt");
        let error =
            decrypt_value(&DekuConfig::default(), &stored).expect_err("no key must not decrypt");
        assert!(error.to_string().contains("no key is configured"));
    }

    #[test]
    fn the_wrong_key_fails_closed() {
        let stored = encrypt_value(&cfg_with_key(&key()), "SECRET=1").expect("encrypt");
        let error = decrypt_value(&cfg_with_key(&"cd".repeat(32)), &stored)
            .expect_err("wrong key must not decrypt");
        assert!(error.to_string().contains("wrong key or corrupted payload"));
    }

    #[test]
    fn tampering_is_detected() {
        let cfg = cfg_with_key(&key());
        let stored = encrypt_value(&cfg, "SECRET=1").expect("encrypt");
        let mut tampered = stored.clone();
        // Flip a character of the base64 body.
        let last = tampered.pop().expect("non-empty");
        tampered.push(if last == 'A' { 'B' } else { 'A' });
        assert!(decrypt_value(&cfg, &tampered).is_err());
    }

    #[test]
    fn view_serializes_the_encryption_state() {
        let view = ConfigVarView {
            app_id: "app-1".to_string(),
            key: "DATABASE_URL".to_string(),
            value: String::new(),
            is_global: false,
            encrypted: true,
            error: Some("no key".to_string()),
        };
        let json = serde_json::to_value(&view).expect("serialize");
        assert_eq!(json["encrypted"], true);
        assert_eq!(json["error"], "no key");
    }

    #[test]
    fn a_different_envelope_is_not_accepted() {
        // A backup envelope base64-encoded under the config prefix must not open.
        let cfg = cfg_with_key(&key());
        let cipher = cfg.at_rest_cipher().expect("resolves").expect("key");
        let backup = cipher
            .seal(crate::crypto::ENVELOPE_MAGIC_BACKUP, b"dump")
            .expect("seal");
        let disguised = format!(
            "{ENVELOPE_PREFIX}{}",
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, backup)
        );
        assert!(decrypt_value(&cfg, &disguised).is_err());
    }
}
