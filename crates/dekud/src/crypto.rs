//! Encryption at rest.
//!
//! Two kinds of sensitive bytes leave the daemon's trust boundary in the clear
//! otherwise: service backups uploaded to an object store, and config var values
//! stored in the state database. Both are sealed with the same AES-256-GCM
//! envelope, keyed by one operator-supplied key.
//!
//! The envelope is self-describing per purpose, so a payload written before a key
//! was configured still reads back unchanged: storage detects the magic, decrypts
//! when it is present, and passes plaintext through untouched.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{anyhow, Result};
use base64::Engine;

/// Envelope magic for a sealed service backup.
pub const ENVELOPE_MAGIC_BACKUP: &[u8] = b"DEKUBK1\n";

/// Envelope magic for a sealed config var value.
///
/// Distinct from the backup magic so the two cannot be confused or swapped: a
/// ciphertext written for one purpose will not open as the other.
pub const ENVELOPE_MAGIC_SECRET: &[u8] = b"DEKUSEC1\n";

/// AES-GCM nonce length in bytes.
const NONCE_LEN: usize = 12;

/// AES-256 key length in bytes.
const KEY_LEN: usize = 32;

/// Minimal sealed payload: nonce plus the 16-byte GCM tag.
const MIN_SEALED_LEN: usize = NONCE_LEN + 16;

/// Label recorded on a backup that was sealed with AES-256-GCM.
pub const ENCRYPTION_LABEL_AES256_GCM: &str = "aes-256-gcm";

/// Label recorded on a backup that was uploaded as-is.
pub const ENCRYPTION_LABEL_NONE: &str = "none";

/// Seals and opens payloads with a single AES-256-GCM key.
pub struct AtRestCipher {
    cipher: Aes256Gcm,
}

impl AtRestCipher {
    /// Build a cipher from raw key bytes.
    pub fn from_key_bytes(key: &[u8]) -> Result<Self> {
        if key.len() != KEY_LEN {
            return Err(anyhow!(
                "backup encryption key must be {KEY_LEN} bytes, got {}",
                key.len()
            ));
        }
        let cipher = Aes256Gcm::new_from_slice(key)
            .map_err(|_| anyhow!("backup encryption key has an invalid length"))?;
        Ok(Self { cipher })
    }

    /// Wrap `plaintext` as `magic || nonce || ciphertext || tag`.
    pub fn seal(&self, magic: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
        use rand::RngExt;

        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::rng().fill(&mut nonce_bytes);
        let nonce = Nonce::try_from(&nonce_bytes[..])
            .map_err(|_| anyhow!("nonce must be {NONCE_LEN} bytes"))?;
        let ciphertext = self
            .cipher
            .encrypt(&nonce, plaintext)
            .map_err(|_| anyhow!("failed to encrypt the backup payload"))?;

        let mut sealed = Vec::with_capacity(magic.len() + NONCE_LEN + ciphertext.len());
        sealed.extend_from_slice(magic);
        sealed.extend_from_slice(&nonce_bytes);
        sealed.extend_from_slice(&ciphertext);
        Ok(sealed)
    }

    /// Open a payload produced by [`AtRestCipher::seal`] for `magic`.
    ///
    /// Fails on tampered or truncated payloads and on the wrong key; GCM
    /// authenticates the ciphertext, so a bad key cannot silently yield garbage.
    pub fn open(&self, magic: &[u8], payload: &[u8]) -> Result<Vec<u8>> {
        if !is_encrypted(payload, magic) {
            return Err(anyhow!("payload does not carry the expected envelope"));
        }
        let body = &payload[magic.len()..];
        if body.len() < MIN_SEALED_LEN {
            return Err(anyhow!(
                "encrypted backup envelope is truncated ({} bytes)",
                body.len()
            ));
        }
        let (nonce_bytes, ciphertext) = body.split_at(NONCE_LEN);
        let nonce =
            Nonce::try_from(nonce_bytes).map_err(|_| anyhow!("nonce must be {NONCE_LEN} bytes"))?;
        self.cipher
            .decrypt(&nonce, ciphertext)
            .map_err(|_| anyhow!("backup could not be decrypted: wrong key or corrupted payload"))
    }
}

impl std::fmt::Debug for AtRestCipher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never let key material reach logs.
        f.write_str("AtRestCipher(<redacted>)")
    }
}

/// Whether `payload` carries the envelope identified by `magic`.
pub fn is_encrypted(payload: &[u8], magic: &[u8]) -> bool {
    payload.starts_with(magic)
}

/// Parse a key supplied as 64 hex characters or a base64-encoded 32 bytes.
pub fn parse_key(raw: &str) -> Result<Vec<u8>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("backup encryption key is empty"));
    }

    if let Ok(decoded) = hex::decode(trimmed) {
        if decoded.len() == KEY_LEN {
            return Ok(decoded);
        }
        return Err(anyhow!(
            "hex backup encryption key must be {} characters ({} bytes), got {}",
            KEY_LEN * 2,
            KEY_LEN,
            trimmed.len()
        ));
    }

    if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(trimmed) {
        if decoded.len() == KEY_LEN {
            return Ok(decoded);
        }
    }
    if let Ok(decoded) = base64::engine::general_purpose::STANDARD_NO_PAD.decode(trimmed) {
        if decoded.len() == KEY_LEN {
            return Ok(decoded);
        }
    }

    Err(anyhow!(
        "backup encryption key must be {} hex characters or base64-encoded {KEY_LEN} bytes",
        KEY_LEN * 2
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        is_encrypted, parse_key, AtRestCipher, ENCRYPTION_LABEL_AES256_GCM, ENCRYPTION_LABEL_NONE,
        ENVELOPE_MAGIC_BACKUP, ENVELOPE_MAGIC_SECRET,
    };
    use base64::Engine;

    fn cipher(byte: u8) -> AtRestCipher {
        AtRestCipher::from_key_bytes(&[byte; 32]).expect("key should be accepted")
    }

    #[test]
    fn seal_then_open_round_trips() {
        let cipher = cipher(7);
        let sealed = cipher
            .seal(ENVELOPE_MAGIC_BACKUP, b"pg_dump output")
            .expect("seal");
        assert!(is_encrypted(&sealed, ENVELOPE_MAGIC_BACKUP));
        assert!(sealed.starts_with(ENVELOPE_MAGIC_BACKUP));
        assert_ne!(&sealed[ENVELOPE_MAGIC_BACKUP.len()..], b"pg_dump output");
        assert_eq!(
            cipher.open(ENVELOPE_MAGIC_BACKUP, &sealed).expect("open"),
            b"pg_dump output"
        );
    }

    #[test]
    fn secret_envelope_round_trips() {
        let cipher = cipher(3);
        let sealed = cipher
            .seal(ENVELOPE_MAGIC_SECRET, b"postgres://user:pw@host/db")
            .expect("seal");
        assert!(is_encrypted(&sealed, ENVELOPE_MAGIC_SECRET));
        assert!(!is_encrypted(&sealed, ENVELOPE_MAGIC_BACKUP));
        assert_eq!(
            cipher.open(ENVELOPE_MAGIC_SECRET, &sealed).expect("open"),
            b"postgres://user:pw@host/db"
        );
    }

    #[test]
    fn envelopes_are_not_interchangeable() {
        let cipher = cipher(3);
        let sealed = cipher.seal(ENVELOPE_MAGIC_BACKUP, b"value").expect("seal");
        // Opening a backup envelope as a secret (or the reverse) must fail rather
        // than misinterpret the bytes.
        assert!(cipher.open(ENVELOPE_MAGIC_SECRET, &sealed).is_err());
    }

    #[test]
    fn seal_uses_a_fresh_nonce_per_call() {
        let cipher = cipher(7);
        let first = cipher
            .seal(ENVELOPE_MAGIC_BACKUP, b"same input")
            .expect("seal");
        let second = cipher
            .seal(ENVELOPE_MAGIC_BACKUP, b"same input")
            .expect("seal");
        assert_ne!(first, second, "nonces must not repeat");
        assert_eq!(
            cipher.open(ENVELOPE_MAGIC_BACKUP, &first).expect("open"),
            b"same input"
        );
        assert_eq!(
            cipher.open(ENVELOPE_MAGIC_BACKUP, &second).expect("open"),
            b"same input"
        );
    }

    #[test]
    fn wrong_key_fails_closed() {
        let sealed = cipher(7)
            .seal(ENVELOPE_MAGIC_BACKUP, b"payload")
            .expect("seal");
        let err = cipher(8)
            .open(ENVELOPE_MAGIC_BACKUP, &sealed)
            .expect_err("wrong key must not decrypt");
        assert!(err.to_string().contains("wrong key or corrupted payload"));
    }

    #[test]
    fn tampered_ciphertext_is_rejected() {
        let cipher = cipher(7);
        let mut sealed = cipher
            .seal(ENVELOPE_MAGIC_BACKUP, b"payload")
            .expect("seal");
        let last = sealed.len() - 1;
        sealed[last] ^= 0x01;
        assert!(
            cipher.open(ENVELOPE_MAGIC_BACKUP, &sealed).is_err(),
            "tampering must be detected"
        );
    }

    #[test]
    fn truncated_envelope_is_rejected() {
        let cipher = cipher(7);
        let sealed = cipher
            .seal(ENVELOPE_MAGIC_BACKUP, b"payload")
            .expect("seal");
        let truncated = &sealed[..ENVELOPE_MAGIC_BACKUP.len() + 4];
        let err = cipher
            .open(ENVELOPE_MAGIC_BACKUP, truncated)
            .expect_err("truncation must fail");
        assert!(err.to_string().contains("truncated"));
    }

    #[test]
    fn plaintext_payload_is_not_an_envelope() {
        let cipher = cipher(7);
        assert!(!is_encrypted(
            b"CREATE TABLE t (id int);",
            ENVELOPE_MAGIC_BACKUP
        ));
        assert!(!is_encrypted(b"LOG_LEVEL=info", ENVELOPE_MAGIC_SECRET));
        assert!(cipher
            .open(ENVELOPE_MAGIC_BACKUP, b"CREATE TABLE t (id int);")
            .is_err());
    }

    #[test]
    fn parse_key_accepts_hex_and_base64() {
        let hex_key = "0".repeat(63) + "1";
        assert_eq!(parse_key(&hex_key).expect("hex"), {
            let mut expected = vec![0u8; 31];
            expected.push(1);
            expected
        });

        let raw = [9u8; 32];
        let base64 = base64::engine::general_purpose::STANDARD.encode(raw);
        assert_eq!(parse_key(&base64).expect("base64"), raw.to_vec());
    }

    #[test]
    fn parse_key_rejects_bad_input() {
        assert!(parse_key("").is_err());
        assert!(parse_key("abcd").is_err());
        assert!(parse_key(&"a".repeat(62)).is_err(), "short hex");
        assert!(parse_key("not a key at all!!").is_err());
    }

    #[test]
    fn from_key_bytes_enforces_length() {
        assert!(AtRestCipher::from_key_bytes(&[0u8; 16]).is_err());
        assert!(AtRestCipher::from_key_bytes(&[0u8; 32]).is_ok());
    }

    #[test]
    fn debug_does_not_leak_key_material() {
        let rendered = format!("{:?}", cipher(0xAB));
        assert_eq!(rendered, "AtRestCipher(<redacted>)");
    }

    #[test]
    fn encryption_labels_are_stable() {
        assert_eq!(ENCRYPTION_LABEL_AES256_GCM, "aes-256-gcm");
        assert_eq!(ENCRYPTION_LABEL_NONE, "none");
    }
}
