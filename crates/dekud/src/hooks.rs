//! Out-of-process lifecycle hooks.
//!
//! Each configured hook receives a JSON POST when a lifecycle event fires. A
//! `pre_build` or `pre_deploy` hook marked `blocking` can fail the deploy; every
//! other combination is advisory and only logs.
//!
//! This is the supported integration point. The in-process cdylib runtime
//! (`crate::plugins`) stays experimental and off by default because loading a
//! foreign shared library can abort the daemon.

use std::time::Duration;

use anyhow::{anyhow, Result};
use deku_core::types::{App, Deployment};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use crate::config::{DekuConfig, HookConfig};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    PreBuild,
    PostBuild,
    PreDeploy,
    PostDeploy,
    DeploySucceeded,
    DeployFailed,
    AppCreated,
    AppDestroyed,
    AlertFired,
    AlertResolved,
}

impl HookEvent {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PreBuild => "pre_build",
            Self::PostBuild => "post_build",
            Self::PreDeploy => "pre_deploy",
            Self::PostDeploy => "post_deploy",
            Self::DeploySucceeded => "deploy.succeeded",
            Self::DeployFailed => "deploy.failed",
            Self::AppCreated => "app.created",
            Self::AppDestroyed => "app.destroyed",
            Self::AlertFired => "alert.fired",
            Self::AlertResolved => "alert.resolved",
        }
    }

    /// Whether a failing hook may fail the operation it observes.
    fn blocking_capable(self) -> bool {
        matches!(self, Self::PreBuild | Self::PreDeploy)
    }
}

/// Event payload data beyond the app and deployment records.
pub struct HookEventData<'a> {
    pub app: &'a App,
    pub deployment: Option<&'a Deployment>,
    /// Event-specific extras, e.g. the builder name or a failure reason.
    pub detail: serde_json::Value,
}

/// Deliver an app-scoped event to every hook that subscribes to it.
///
/// Returns an error only when a blocking hook fails a blocking-capable event.
pub async fn fire(cfg: &DekuConfig, event: HookEvent, data: HookEventData<'_>) -> Result<()> {
    deliver(cfg, event, Some(data.app), data.deployment, data.detail).await
}

/// Deliver a host-scoped event, such as an alert, that has no app.
pub async fn fire_host(
    cfg: &DekuConfig,
    event: HookEvent,
    detail: serde_json::Value,
) -> Result<()> {
    deliver(cfg, event, None, None, detail).await
}

async fn deliver(
    cfg: &DekuConfig,
    event: HookEvent,
    app: Option<&App>,
    deployment: Option<&Deployment>,
    detail: serde_json::Value,
) -> Result<()> {
    let hooks: Vec<&HookConfig> = cfg
        .hooks
        .iter()
        .filter(|hook| delivers(hook, event))
        .collect();
    if hooks.is_empty() {
        return Ok(());
    }

    let payload = serde_json::json!({
        "event": event.as_str(),
        "timestamp": chrono::Utc::now(),
        "deku_version": deku_core::version::release_version(),
        "app": app.map(|app| serde_json::json!({ "id": app.id, "name": app.name })),
        "deployment": deployment.map(|deployment| {
            serde_json::json!({
                "id": deployment.id,
                "image_tag": deployment.image_tag,
                "status": deployment.status,
            })
        }),
        "detail": detail,
    });
    let body = serde_json::to_vec(&payload)?;

    let mut blocking_failure: Option<anyhow::Error> = None;
    for hook in hooks {
        match post(hook, event, &body).await {
            Ok(()) => {
                tracing::debug!(url = %hook.url, event = event.as_str(), "hook delivered");
            }
            Err(error) if hook.blocking && event.blocking_capable() => {
                tracing::error!(
                    url = %hook.url,
                    event = event.as_str(),
                    "blocking hook failed: {error}"
                );
                blocking_failure = Some(anyhow!(
                    "{} hook '{}' failed: {error}",
                    event.as_str(),
                    hook.url
                ));
            }
            Err(error) => {
                tracing::warn!(
                    url = %hook.url,
                    event = event.as_str(),
                    "hook failed: {error}"
                );
            }
        }
    }

    match blocking_failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn delivers(hook: &HookConfig, event: HookEvent) -> bool {
    match hook.events.as_ref() {
        Some(events) => events.iter().any(|name| name == event.as_str()),
        None => true,
    }
}

async fn post(hook: &HookConfig, event: HookEvent, body: &[u8]) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()?;

    let mut request = client
        .post(&hook.url)
        .header("content-type", "application/json")
        .header("x-deku-event", event.as_str())
        // Lets a receiver deduplicate if it retries on its own side.
        .header("x-deku-delivery", uuid::Uuid::new_v4().to_string());

    if let Some(secret) = hook.secret.as_deref().filter(|value| !value.is_empty()) {
        request = request.header(
            "x-deku-signature",
            format!("sha256={}", sign(secret, body)?),
        );
    }

    let response = request.body(body.to_vec()).send().await?;
    if !response.status().is_success() {
        anyhow::bail!("endpoint returned HTTP {}", response.status());
    }
    Ok(())
}

/// HMAC-SHA256 signature over the exact request body.
pub fn sign(secret: &str, body: &[u8]) -> Result<String> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .map_err(|error| anyhow!("invalid hook secret: {error}"))?;
    mac.update(body);
    Ok(hex::encode(mac.finalize().into_bytes()))
}

#[cfg(test)]
mod tests {
    use super::{delivers, sign, HookConfig, HookEvent};

    fn hook(events: Option<&[&str]>, blocking: bool) -> HookConfig {
        HookConfig {
            url: "https://hooks.example/deku".to_string(),
            secret: None,
            events: events.map(|list| list.iter().map(|name| name.to_string()).collect()),
            blocking,
        }
    }

    #[test]
    fn a_hook_without_an_event_filter_receives_everything() {
        let hook = hook(None, false);
        assert!(delivers(&hook, HookEvent::PreDeploy));
        assert!(delivers(&hook, HookEvent::DeployFailed));
    }

    #[test]
    fn an_event_filter_limits_delivery() {
        let hook = hook(Some(&["pre_deploy", "deploy.failed"]), true);
        assert!(delivers(&hook, HookEvent::PreDeploy));
        assert!(delivers(&hook, HookEvent::DeployFailed));
        assert!(!delivers(&hook, HookEvent::PostBuild));
        assert!(!delivers(&hook, HookEvent::AppCreated));
    }

    #[test]
    fn only_pre_events_can_block() {
        assert!(HookEvent::PreDeploy.blocking_capable());
        assert!(HookEvent::PreBuild.blocking_capable());
        assert!(!HookEvent::PostDeploy.blocking_capable());
        assert!(!HookEvent::DeployFailed.blocking_capable());
        assert!(!HookEvent::AppCreated.blocking_capable());
    }

    #[test]
    fn event_names_are_stable() {
        assert_eq!(HookEvent::PreDeploy.as_str(), "pre_deploy");
        assert_eq!(HookEvent::DeploySucceeded.as_str(), "deploy.succeeded");
        assert_eq!(HookEvent::AppDestroyed.as_str(), "app.destroyed");
    }

    /// RFC 4231 test case 2: key "Jefe", data "what do ya want for nothing?".
    #[test]
    fn signature_matches_rfc_4231() {
        let signature = sign("Jefe", b"what do ya want for nothing?").expect("sign");
        assert_eq!(
            signature,
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn signature_depends_on_the_exact_body() {
        let first = sign("secret", b"{\"event\":\"pre_deploy\"}").expect("sign");
        let second = sign("secret", b"{\"event\":\"pre_deploy\" }").expect("sign");
        assert_ne!(first, second);
        assert_eq!(first.len(), 64);
    }
}
