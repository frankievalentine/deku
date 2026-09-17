//! Fixed-rule alert evaluation.
//!
//! A background watcher periodically turns observable state into alerts: TLS
//! certificates close to expiry, apps that were deployed but have nothing
//! serving, failing or overdue backups, an unreachable object store, and a
//! filling filesystem. Alerts are stored so they are visible after the fact and
//! delivered once per state change over the HTTP hook surface
//! (`alert.fired` / `alert.resolved`).
//!
//! Alerts are intentionally not configurable here. Rules are code, so their
//! meaning cannot drift from the checks the daemon actually performs; the
//! knobs that matter (interval, disk thresholds, on/off) live in `[alerts]`.

use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use tokio::time::interval;
use tracing::{info, warn};

use crate::api::SharedState;
use crate::db::queries;
use crate::hooks::{self, HookEvent};
use crate::services::letsencrypt::{self, CertLifecycle};

/// Certificate or key file is missing while TLS is enabled.
pub const RULE_CERTIFICATE_MISSING: &str = "certificate_missing";
/// Certificate is past its expiry.
pub const RULE_CERTIFICATE_EXPIRED: &str = "certificate_expired";
/// Certificate is inside the renewal window.
pub const RULE_CERTIFICATE_EXPIRING: &str = "certificate_expiring";
/// The app has been deployed but no web container is running.
pub const RULE_APP_NO_RUNNING_CONTAINERS: &str = "app_no_running_containers";
/// The most recent scheduled backup failed.
pub const RULE_BACKUP_FAILED: &str = "backup_failed";
/// A scheduled backup is past its due time.
pub const RULE_BACKUP_OVERDUE: &str = "backup_schedule_overdue";
/// The configured object store could not be reached.
pub const RULE_OBJECT_STORE_UNREACHABLE: &str = "object_store_unreachable";
/// Filesystem usage is above the configured threshold.
pub const RULE_DISK_HIGH: &str = "disk_usage_high";

pub const SEVERITY_WARNING: &str = "warning";
pub const SEVERITY_CRITICAL: &str = "critical";

/// Scope used by alerts that are not tied to one app or service.
pub const HOST_SCOPE: &str = "host";

/// One rule finding, before it is reconciled with stored alerts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertDraft {
    pub rule: &'static str,
    pub severity: &'static str,
    /// Stable identity, e.g. `app:web` or `service:pg`.
    pub scope: String,
    pub subject: String,
    pub message: String,
}

impl AlertDraft {
    fn new(
        rule: &'static str,
        severity: &'static str,
        scope: impl Into<String>,
        subject: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            rule,
            severity,
            scope: scope.into(),
            subject: subject.into(),
            message: message.into(),
        }
    }
}

/// Spawn the alert watcher, unless it is disabled.
pub fn spawn(state: SharedState) {
    if !state.config.alerts.enabled {
        info!("alert watcher disabled by configuration");
        return;
    }

    let interval_secs = state.config.alerts.interval_secs.max(10);
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(interval_secs));
        loop {
            ticker.tick().await;
            if let Err(error) = run_once(&state).await {
                warn!(error = %error, "alert evaluation failed");
            }
        }
    });
}

/// Evaluate every rule once and reconcile the result with stored alerts.
pub async fn run_once(state: &SharedState) -> Result<()> {
    let drafts = evaluate(state).await?;
    reconcile(state, drafts).await
}

/// Collect the current findings. Pure with respect to stored alert state, so it
/// can be exercised without touching the alerts table.
pub async fn evaluate(state: &SharedState) -> Result<Vec<AlertDraft>> {
    let mut drafts = Vec::new();
    let apps = queries::list_apps(&state.pool).await?;

    // Certificates.
    for app in apps.iter().filter(|app| app.tls_enabled) {
        let scope = format!("app:{}", app.name);
        match letsencrypt::status(&state.pool, &app.name).await {
            Ok(status) => match status.lifecycle {
                CertLifecycle::Missing => drafts.push(AlertDraft::new(
                    RULE_CERTIFICATE_MISSING,
                    SEVERITY_CRITICAL,
                    scope,
                    &app.name,
                    format!(
                        "TLS is enabled for '{}' but the certificate or key file is missing",
                        app.name
                    ),
                )),
                CertLifecycle::Expired => drafts.push(AlertDraft::new(
                    RULE_CERTIFICATE_EXPIRED,
                    SEVERITY_CRITICAL,
                    scope,
                    &app.name,
                    format!(
                        "certificate for '{}' expired {} day(s) ago",
                        app.name,
                        status.days_remaining.unwrap_or_default().abs()
                    ),
                )),
                CertLifecycle::Expiring => drafts.push(AlertDraft::new(
                    RULE_CERTIFICATE_EXPIRING,
                    SEVERITY_WARNING,
                    scope,
                    &app.name,
                    format!(
                        "certificate for '{}' expires in {} day(s)",
                        app.name,
                        status.days_remaining.unwrap_or_default()
                    ),
                )),
                CertLifecycle::Ok => {}
                CertLifecycle::Unknown => drafts.push(AlertDraft::new(
                    RULE_CERTIFICATE_MISSING,
                    SEVERITY_WARNING,
                    scope,
                    &app.name,
                    format!("certificate for '{}' could not be inspected", app.name),
                )),
            },
            Err(error) => drafts.push(AlertDraft::new(
                RULE_CERTIFICATE_MISSING,
                SEVERITY_WARNING,
                scope,
                &app.name,
                format!("certificate for '{}' could not be read: {error}", app.name),
            )),
        }
    }

    // Apps that were deployed but have nothing serving.
    for name in queries::list_apps_without_running_web_containers(&state.pool).await? {
        drafts.push(AlertDraft::new(
            RULE_APP_NO_RUNNING_CONTAINERS,
            SEVERITY_CRITICAL,
            format!("app:{name}"),
            &name,
            format!("app '{name}' has no running web container"),
        ));
    }

    // Backup schedules.
    let now = Utc::now();
    for (name, _plugin, last_status, next_run_at) in
        queries::list_backup_schedules_for_alerts(&state.pool).await?
    {
        if last_status.as_deref() == Some("error") {
            drafts.push(AlertDraft::new(
                RULE_BACKUP_FAILED,
                SEVERITY_WARNING,
                format!("service:{name}"),
                &name,
                format!("the last scheduled backup for '{name}' failed"),
            ));
        }
        if let Some(next_run) = next_run_at {
            if next_run < now {
                drafts.push(AlertDraft::new(
                    RULE_BACKUP_OVERDUE,
                    SEVERITY_WARNING,
                    format!("service:{name}"),
                    &name,
                    format!(
                        "the scheduled backup for '{name}' is overdue (was due {})",
                        next_run.to_rfc3339()
                    ),
                ));
            }
        }
    }

    // Object store reachability.
    if let Some(object_store) = state.config.object_store.as_ref() {
        if let Err(error) = crate::objectstore::test_config(object_store).await {
            drafts.push(AlertDraft::new(
                RULE_OBJECT_STORE_UNREACHABLE,
                SEVERITY_CRITICAL,
                HOST_SCOPE,
                HOST_SCOPE,
                format!("object store is unreachable: {error}"),
            ));
        }
    }

    // Disk usage.
    if let Some(draft) = disk_alert(&state.config.data_dir, &state.config.alerts) {
        drafts.push(draft);
    }

    Ok(drafts)
}

/// Warn or fail on a filling filesystem, using `statvfs` on the data directory.
fn disk_alert(data_dir: &std::path::Path, cfg: &crate::config::AlertsConfig) -> Option<AlertDraft> {
    let used_percent = filesystem_used_percent(data_dir)?;
    let severity = if used_percent >= cfg.disk_critical_percent {
        SEVERITY_CRITICAL
    } else if used_percent >= cfg.disk_warn_percent {
        SEVERITY_WARNING
    } else {
        return None;
    };

    Some(AlertDraft::new(
        RULE_DISK_HIGH,
        severity,
        HOST_SCOPE,
        HOST_SCOPE,
        format!(
            "filesystem holding {} is {used_percent}% full",
            data_dir.display()
        ),
    ))
}

#[cfg(unix)]
fn filesystem_used_percent(path: &std::path::Path) -> Option<u8> {
    use std::ffi::CString;

    let c_path = CString::new(path.to_string_lossy().as_bytes()).ok()?;
    // SAFETY: `statvfs` writes into a fully initialized struct we own, and the
    // path is a valid NUL-terminated C string for the duration of the call.
    let stats = unsafe {
        let mut stats: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr(), &mut stats) != 0 {
            return None;
        }
        stats
    };

    let blocks = stats.f_blocks as u128;
    if blocks == 0 {
        return None;
    }
    let free = stats.f_bfree as u128;
    let used = blocks.saturating_sub(free);
    Some(((used * 100) / blocks) as u8)
}

#[cfg(not(unix))]
fn filesystem_used_percent(_path: &std::path::Path) -> Option<u8> {
    None
}

/// Store the findings, notify on transitions, and resolve alerts that cleared.
pub async fn reconcile(state: &SharedState, drafts: Vec<AlertDraft>) -> Result<()> {
    for draft in &drafts {
        let (alert, is_new) = queries::upsert_alert(
            &state.pool,
            draft.rule,
            draft.severity,
            &draft.scope,
            &draft.subject,
            &draft.message,
        )
        .await?;

        if is_new {
            info!(rule = draft.rule, scope = %draft.scope, severity = draft.severity, "alert fired");
            notify(state, HookEvent::AlertFired, &alert).await;
        }
    }

    let active = queries::list_active_alerts(&state.pool).await?;
    for alert in active {
        let still_present = drafts
            .iter()
            .any(|draft| draft.rule == alert.rule && draft.scope == alert.scope);
        if still_present {
            continue;
        }

        if let Some(resolved) = queries::resolve_alert(&state.pool, &alert.id).await? {
            info!(rule = resolved.rule, scope = %resolved.scope, "alert resolved");
            notify(state, HookEvent::AlertResolved, &resolved).await;
        }
    }

    Ok(())
}

async fn notify(state: &SharedState, event: HookEvent, alert: &queries::Alert) {
    let detail = serde_json::json!({
        "alert": {
            "id": alert.id,
            "rule": alert.rule,
            "severity": alert.severity,
            "scope": alert.scope,
            "subject": alert.subject,
            "message": alert.message,
            "first_seen_at": alert.first_seen_at,
            "last_seen_at": alert.last_seen_at,
            "resolved_at": alert.resolved_at,
        }
    });

    if let Err(error) = hooks::fire_host(&state.config, event, detail).await {
        warn!(error = %error, event = event.as_str(), "alert hook delivery failed");
    }
}

#[cfg(test)]
mod tests {
    use super::{disk_alert, AlertDraft, RULE_DISK_HIGH, SEVERITY_CRITICAL, SEVERITY_WARNING};
    use crate::config::AlertsConfig;

    fn alerts_config(warn: u8, critical: u8) -> AlertsConfig {
        AlertsConfig {
            enabled: true,
            interval_secs: 300,
            disk_warn_percent: warn,
            disk_critical_percent: critical,
        }
    }

    #[test]
    fn drafts_are_value_equal() {
        let first = AlertDraft::new("rule", "warning", "host", "host", "message");
        let second = AlertDraft::new("rule", "warning", "host", "host", "message");
        assert_eq!(first, second);
    }

    #[cfg(unix)]
    #[test]
    fn disk_alert_escalates_with_usage() {
        // Thresholds below any real filesystem's usage force an alert.
        let warn = disk_alert(std::path::Path::new("/"), &alerts_config(0, 101));
        assert_eq!(
            warn.as_ref().map(|draft| draft.severity),
            Some(SEVERITY_WARNING)
        );
        assert_eq!(warn.as_ref().map(|draft| draft.rule), Some(RULE_DISK_HIGH));

        let critical = disk_alert(std::path::Path::new("/"), &alerts_config(0, 0));
        assert_eq!(
            critical.as_ref().map(|draft| draft.severity),
            Some(SEVERITY_CRITICAL)
        );
    }

    #[cfg(unix)]
    #[test]
    fn disk_alert_is_absent_when_thresholds_are_high() {
        assert!(disk_alert(std::path::Path::new("/"), &alerts_config(99, 100)).is_none());
    }

    #[test]
    fn disk_alert_survives_a_missing_path() {
        let missing = std::path::Path::new("/definitely/not/here");
        let _ = disk_alert(missing, &alerts_config(0, 0));
    }
}
