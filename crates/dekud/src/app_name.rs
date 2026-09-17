//! App-name validation.
//!
//! A name becomes a path segment (`<name>.conf`, `<name>.htpasswd`, `<name>.git`)
//! and appears in URLs, so it must not be able to escape a directory or look like
//! a command-line flag. Existing rows are never re-validated; this only guards
//! names being written.

/// Longest accepted app name, matching common DNS-label limits.
const MAX_LEN: usize = 63;

pub fn validate(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("app name must not be empty".to_string());
    }
    if name.len() > MAX_LEN {
        return Err(format!("app name must be at most {MAX_LEN} characters"));
    }
    if name.starts_with('-') {
        return Err("app name must not start with '-'".to_string());
    }
    if name.contains("..") {
        return Err("app name must not contain '..'".to_string());
    }
    if name.starts_with('.') {
        // `<name>.conf` would become a dotfile, which Angie's config glob skips,
        // so the vhost would never load.
        return Err("app name must not start with '.'".to_string());
    }
    if let Some(character) = name
        .chars()
        .find(|c| !matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.'))
    {
        return Err(format!(
            "app name must use only letters, digits, '-', '_', and '.' (found {character:?})"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate;

    #[test]
    fn accepts_ordinary_names() {
        for name in ["app", "my-app", "my_app", "app.v2", "App1", "a"] {
            assert!(validate(name).is_ok(), "{name} should be accepted");
        }
    }

    #[test]
    fn rejects_path_traversal_and_separators() {
        for name in ["..", "../etc/passwd", "a/../b", "a/b", "a\\b"] {
            let error = validate(name).expect_err(name);
            assert!(
                error.contains("..") || error.contains("letters"),
                "{name}: {error}"
            );
        }
    }

    #[test]
    fn rejects_leading_dot_names() {
        let error = validate(".hidden").expect_err(".hidden");
        assert!(error.contains("start with"), "{error}");
    }

    #[test]
    fn rejects_empty_leading_dash_and_whitespace() {
        assert!(validate("").is_err());
        assert!(validate("-flag").is_err());
        assert!(validate("my app").is_err());
        assert!(validate("my\tapp").is_err());
    }

    #[test]
    fn rejects_overlong_names() {
        assert!(validate(&"a".repeat(64)).is_err());
        assert!(validate(&"a".repeat(63)).is_ok());
    }
}
