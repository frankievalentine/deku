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

/// Environment slugs are stricter than app names.
///
/// A slug is used in generated hostnames (`<app>-<slug>.<domain>`) and in the
/// per-environment config file name, so it stays inside DNS-label territory:
/// lowercase letters, digits, and '-'.
pub fn validate_slug(slug: &str) -> Result<(), String> {
    validate(slug)?;
    if let Some(character) = slug
        .chars()
        .find(|c| !matches!(c, 'a'..='z' | '0'..='9' | '-'))
    {
        return Err(format!(
            "environment slug must use only lowercase letters, digits, and '-' (found {character:?})"
        ));
    }
    Ok(())
}

/// Derive a slug from a display name: `Staging EU` becomes `staging-eu`.
pub fn slugify(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut last_was_dash = false;
    for character in name.chars() {
        let character = character.to_ascii_lowercase();
        if matches!(character, 'a'..='z' | '0'..='9') {
            slug.push(character);
            last_was_dash = false;
        } else if !last_was_dash && !slug.is_empty() {
            slug.push('-');
            last_was_dash = true;
        }
    }
    slug.trim_end_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::{slugify, validate_slug};

    #[test]
    fn slugs_reject_uppercase_and_underscores() {
        assert!(validate_slug("staging").is_ok());
        assert!(validate_slug("pr-123").is_ok());
        assert!(validate_slug("Staging").is_err());
        assert!(validate_slug("staging_eu").is_err());
        assert!(validate_slug("staging.eu").is_err());
        assert!(validate_slug("").is_err());
        assert!(validate_slug("-leading").is_err());
        assert!(validate_slug("../etc").is_err());
    }

    #[test]
    fn slugify_normalizes_a_display_name() {
        assert_eq!(slugify("Staging EU"), "staging-eu");
        assert_eq!(slugify("  PR 123  "), "pr-123");
        assert_eq!(slugify("Feature//Branch"), "feature-branch");
        assert_eq!(slugify("already-fine"), "already-fine");
        assert_eq!(slugify("--"), "");
    }

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
