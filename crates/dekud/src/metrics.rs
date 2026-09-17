//! Prometheus metrics.
//!
//! `/api/metrics` renders a snapshot of daemon state in the Prometheus text
//! exposition format, so an operator can scrape Deku with the monitoring stack
//! they already run. Rendering is a pure function of a snapshot, which keeps the
//! format testable without a database.

use anyhow::Result;
use chrono::Utc;
use sqlx::SqlitePool;

use crate::db::queries;

/// Everything the exposed metrics are derived from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricsSnapshot {
    pub version: String,
    pub apps_total: i64,
    pub apps_deployed: i64,
    pub apps_tls_enabled: i64,
    pub services_total: i64,
    pub services_by_plugin: Vec<(String, i64)>,
    pub containers_running: i64,
    pub deployments_live: i64,
    pub service_backups_total: i64,
    pub backup_schedules_overdue: i64,
    pub alerts_warning: i64,
    pub alerts_critical: i64,
}

/// Gather the current metric values.
pub async fn snapshot(pool: &SqlitePool) -> Result<MetricsSnapshot> {
    let services_by_plugin = queries::count_services_by_plugin(pool).await?;
    let alert_counts = queries::count_active_alerts_by_severity(pool).await?;
    let severity_count = |wanted: &str| {
        alert_counts
            .iter()
            .filter(|(severity, _)| severity == wanted)
            .map(|(_, count)| *count)
            .sum()
    };

    Ok(MetricsSnapshot {
        version: deku_core::version::release_version().to_string(),
        apps_total: count(pool, "SELECT COUNT(*) FROM apps").await?,
        apps_deployed: count(pool, "SELECT COUNT(*) FROM apps WHERE status = 'deployed'").await?,
        apps_tls_enabled: count(pool, "SELECT COUNT(*) FROM apps WHERE tls_enabled = 1").await?,
        services_total: count(pool, "SELECT COUNT(*) FROM services").await?,
        services_by_plugin,
        containers_running: count(
            pool,
            "SELECT COUNT(*) FROM containers WHERE status = 'running'",
        )
        .await?,
        deployments_live: count(
            pool,
            "SELECT COUNT(*) FROM deployments WHERE status = 'live'",
        )
        .await?,
        service_backups_total: count(pool, "SELECT COUNT(*) FROM service_backups").await?,
        backup_schedules_overdue: queries::count_overdue_backup_schedules(pool, Utc::now()).await?,
        alerts_warning: severity_count(crate::alerts::SEVERITY_WARNING),
        alerts_critical: severity_count(crate::alerts::SEVERITY_CRITICAL),
    })
}

async fn count(pool: &SqlitePool, query: &'static str) -> Result<i64> {
    Ok(sqlx::query_scalar::<_, i64>(query).fetch_one(pool).await?)
}

/// Render a snapshot in the Prometheus text exposition format.
pub fn render(snapshot: &MetricsSnapshot) -> String {
    let mut out = String::new();

    gauge(
        &mut out,
        "deku_build_info",
        "Deku release version, always 1.",
        Some(&[("version", snapshot.version.as_str())]),
        "1",
    );
    gauge(
        &mut out,
        "deku_apps_total",
        "Apps managed by this daemon.",
        None,
        &snapshot.apps_total.to_string(),
    );
    gauge(
        &mut out,
        "deku_apps_deployed",
        "Apps whose last deployment is live.",
        None,
        &snapshot.apps_deployed.to_string(),
    );
    gauge(
        &mut out,
        "deku_apps_tls_enabled",
        "Apps with TLS enabled.",
        None,
        &snapshot.apps_tls_enabled.to_string(),
    );
    gauge(
        &mut out,
        "deku_services_total",
        "Managed services.",
        None,
        &snapshot.services_total.to_string(),
    );
    header(
        &mut out,
        "deku_services_by_plugin",
        "Managed services per plugin.",
    );
    for (plugin, count) in &snapshot.services_by_plugin {
        out.push_str(&metric_line(
            "deku_services_by_plugin",
            Some(&[("plugin", plugin.as_str())]),
            &count.to_string(),
        ));
    }
    gauge(
        &mut out,
        "deku_containers_running",
        "Running containers across all apps.",
        None,
        &snapshot.containers_running.to_string(),
    );
    gauge(
        &mut out,
        "deku_deployments_live",
        "Deployments currently serving traffic.",
        None,
        &snapshot.deployments_live.to_string(),
    );
    gauge(
        &mut out,
        "deku_service_backups_total",
        "Stored service backups.",
        None,
        &snapshot.service_backups_total.to_string(),
    );
    gauge(
        &mut out,
        "deku_backup_schedules_overdue",
        "Backup schedules past their due time.",
        None,
        &snapshot.backup_schedules_overdue.to_string(),
    );
    header(
        &mut out,
        "deku_alerts_active",
        "Active alerts per severity.",
    );
    out.push_str(&metric_line(
        "deku_alerts_active",
        Some(&[("severity", crate::alerts::SEVERITY_WARNING)]),
        &snapshot.alerts_warning.to_string(),
    ));
    out.push_str(&metric_line(
        "deku_alerts_active",
        Some(&[("severity", crate::alerts::SEVERITY_CRITICAL)]),
        &snapshot.alerts_critical.to_string(),
    ));

    out
}

fn gauge(out: &mut String, name: &str, help: &str, labels: Option<&[(&str, &str)]>, value: &str) {
    header(out, name, help);
    out.push_str(&metric_line(name, labels, value));
}

/// Write the HELP and TYPE lines for a metric.
///
/// Metrics whose samples all carry labels must not also emit an unlabeled
/// sample, so they call this instead of `gauge`.
fn header(out: &mut String, name: &str, help: &str) {
    out.push_str(&format!("# HELP {name} {help}\n# TYPE {name} gauge\n"));
}

fn metric_line(name: &str, labels: Option<&[(&str, &str)]>, value: &str) -> String {
    match labels {
        Some(labels) if !labels.is_empty() => {
            let rendered: Vec<String> = labels
                .iter()
                .map(|(key, value)| format!("{key}=\"{}\"", escape_label_value(value)))
                .collect();
            format!("{name}{{{}}} {value}\n", rendered.join(","))
        }
        _ => format!("{name} {value}\n"),
    }
}

/// Escape a label value per the Prometheus exposition format.
fn escape_label_value(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::{escape_label_value, render, MetricsSnapshot};

    fn snapshot() -> MetricsSnapshot {
        MetricsSnapshot {
            version: "1.2.3".to_string(),
            apps_total: 3,
            apps_deployed: 2,
            apps_tls_enabled: 1,
            services_total: 2,
            services_by_plugin: vec![("postgres".to_string(), 2)],
            containers_running: 4,
            deployments_live: 2,
            service_backups_total: 5,
            backup_schedules_overdue: 1,
            alerts_warning: 1,
            alerts_critical: 0,
        }
    }

    #[test]
    fn renders_help_and_type_for_every_metric() {
        let output = render(&snapshot());
        for name in [
            "deku_build_info",
            "deku_apps_total",
            "deku_apps_deployed",
            "deku_apps_tls_enabled",
            "deku_services_total",
            "deku_services_by_plugin",
            "deku_containers_running",
            "deku_deployments_live",
            "deku_service_backups_total",
            "deku_backup_schedules_overdue",
            "deku_alerts_active",
        ] {
            assert!(
                output.contains(&format!("# TYPE {name} gauge")),
                "missing TYPE line for {name}"
            );
            assert!(
                output.contains(&format!("# HELP {name} ")),
                "missing HELP line for {name}"
            );
        }
    }

    #[test]
    fn renders_values_and_labels() {
        let output = render(&snapshot());
        assert!(output.contains("deku_build_info{version=\"1.2.3\"} 1"));
        assert!(output.contains("deku_apps_total 3"));
        assert!(output.contains("deku_services_by_plugin{plugin=\"postgres\"} 2"));
        assert!(output.contains("deku_alerts_active{severity=\"warning\"} 1"));
        assert!(output.contains("deku_alerts_active{severity=\"critical\"} 0"));
    }

    #[test]
    fn label_values_are_escaped() {
        assert_eq!(escape_label_value("plain"), "plain");
        assert_eq!(escape_label_value("a\"b"), "a\\\"b");
        assert_eq!(escape_label_value("a\\b"), "a\\\\b");
        assert_eq!(escape_label_value("a\nb"), "a\\nb");
    }

    #[test]
    fn labeled_metrics_have_no_unlabeled_sample() {
        let output = render(&snapshot());
        for name in ["deku_alerts_active", "deku_services_by_plugin"] {
            for line in output.lines().filter(|line| line.starts_with(name)) {
                assert!(
                    line.starts_with(&format!("{name}{{")),
                    "labeled metric emitted an unlabeled sample: {line}"
                );
            }
        }
    }

    #[test]
    fn every_value_line_is_parseable() {
        let output = render(&snapshot());
        for line in output.lines() {
            if line.starts_with('#') {
                continue;
            }
            let (_, value) = line.rsplit_once(' ').expect("value separated by a space");
            assert!(
                value.parse::<f64>().is_ok(),
                "metric value should be numeric: {line}"
            );
        }
    }
}
