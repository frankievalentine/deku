use std::{
    cmp::Ordering,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Context, Result};
use deku_core::version::{compare_release_versions, release_version};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

const CACHE_TTL: Duration = Duration::from_secs(300);
const GITHUB_LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/frankievalentine/deku/releases/latest";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VersionStatus {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CachedVersionStatus {
    pub checked_at: Instant,
    pub status: VersionStatus,
}

#[derive(Debug, Deserialize)]
struct GithubReleasePayload {
    tag_name: String,
}

pub async fn resolve_version_status(
    cache: &RwLock<Option<CachedVersionStatus>>,
) -> Result<VersionStatus> {
    {
        let cached = cache.read().await;
        if let Some(cached) = cached.as_ref() {
            if cached.checked_at.elapsed() < CACHE_TTL {
                return Ok(cached.status.clone());
            }
        }
    }

    let status = build_version_status(release_version(), fetch_latest_release_version().await);

    let mut cached = cache.write().await;
    *cached = Some(CachedVersionStatus {
        checked_at: Instant::now(),
        status: status.clone(),
    });

    Ok(status)
}

async fn fetch_latest_release_version() -> Result<String> {
    let client = reqwest::Client::new();
    let response = client
        .get(GITHUB_LATEST_RELEASE_URL)
        .header(
            reqwest::header::USER_AGENT,
            format!("dekud/{}", release_version()),
        )
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .context("failed to check GitHub releases")?;

    let response = response
        .error_for_status()
        .context("GitHub releases request failed")?;
    let payload: GithubReleasePayload = response
        .json()
        .await
        .context("failed to parse GitHub release metadata")?;

    if payload.tag_name.trim().is_empty() {
        return Err(anyhow!("GitHub latest release did not include a tag name"));
    }

    Ok(payload.tag_name)
}

fn build_version_status(current_version: &str, latest_release: Result<String>) -> VersionStatus {
    match latest_release {
        Ok(latest_version) => {
            let update_available = matches!(
                compare_release_versions(&latest_version, current_version),
                Some(Ordering::Greater)
            );

            VersionStatus {
                current_version: current_version.to_string(),
                latest_version: Some(latest_version),
                update_available,
                status: "ok".to_string(),
                error: None,
            }
        }
        Err(error) => VersionStatus {
            current_version: current_version.to_string(),
            latest_version: None,
            update_available: false,
            status: "error".to_string(),
            error: Some(error.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::build_version_status;

    #[test]
    fn marks_newer_release_as_available() {
        let status = build_version_status("v0.1.8", Ok("v0.1.9".to_string()));
        assert_eq!(status.current_version, "v0.1.8");
        assert_eq!(status.latest_version.as_deref(), Some("v0.1.9"));
        assert!(status.update_available);
        assert_eq!(status.status, "ok");
        assert_eq!(status.error, None);
    }

    #[test]
    fn handles_equal_release_versions() {
        let status = build_version_status("v0.1.8", Ok("v0.1.8".to_string()));
        assert!(!status.update_available);
        assert_eq!(status.status, "ok");
    }

    #[test]
    fn degrades_cleanly_on_release_lookup_failures() {
        let status = build_version_status("v0.1.8", Err(anyhow::anyhow!("network timeout")));
        assert_eq!(status.latest_version, None);
        assert!(!status.update_available);
        assert_eq!(status.status, "error");
        assert_eq!(status.error.as_deref(), Some("network timeout"));
    }
}
