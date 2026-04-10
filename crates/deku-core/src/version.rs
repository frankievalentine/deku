use std::cmp::Ordering;

const FALLBACK_RELEASE_VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"));

pub fn release_version() -> &'static str {
    option_env!("DEKU_RELEASE_VERSION").unwrap_or(FALLBACK_RELEASE_VERSION)
}

pub fn compare_release_versions(left: &str, right: &str) -> Option<Ordering> {
    let left = parse_release_version(left)?;
    let right = parse_release_version(right)?;
    Some(left.cmp(&right))
}

fn parse_release_version(value: &str) -> Option<Vec<u64>> {
    let trimmed = value.trim();
    let version = trimmed.strip_prefix('v').unwrap_or(trimmed);
    let mut segments = Vec::new();

    for segment in version.split('.') {
        if segment.is_empty() {
            return None;
        }

        segments.push(segment.parse::<u64>().ok()?);
    }

    if segments.is_empty() {
        None
    } else {
        Some(segments)
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use super::compare_release_versions;

    #[test]
    fn compares_tagged_versions() {
        assert_eq!(
            compare_release_versions("v0.1.8", "v0.1.7"),
            Some(Ordering::Greater)
        );
        assert_eq!(
            compare_release_versions("v0.1.8", "v0.1.8"),
            Some(Ordering::Equal)
        );
        assert_eq!(
            compare_release_versions("v0.1.8", "v0.2.0"),
            Some(Ordering::Less)
        );
    }

    #[test]
    fn supports_untagged_fallback_versions() {
        assert_eq!(
            compare_release_versions("0.1.8", "v0.1.7"),
            Some(Ordering::Greater)
        );
    }

    #[test]
    fn rejects_invalid_versions() {
        assert_eq!(compare_release_versions("latest", "v0.1.8"), None);
        assert_eq!(compare_release_versions("v0.1.x", "v0.1.8"), None);
    }
}
