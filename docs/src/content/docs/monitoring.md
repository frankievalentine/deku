---
title: Monitoring and alerts
description: Scrape Deku with Prometheus and let the fixed-rule watcher tell you what needs attention.
---

Deku exposes two surfaces for watching a host: a Prometheus metrics endpoint for dashboards and
graphs, and an alert watcher for conditions that need a human.

## Metrics

`GET /api/metrics` renders daemon state in the Prometheus text exposition format
(`text/plain; version=0.0.4`). Scrape it over the daemon socket or the dashboard port; when you use
the HTTP port, send a dashboard token as a bearer token like any other API call.

```yaml
scrape_configs:
  - job_name: deku
    metrics_path: /api/metrics
    authorization:
      credentials_file: /etc/deku/scrape.token
    static_configs:
      - targets: ["deku.internal:2810"]
```

Exposed series:

| Metric | Meaning |
| --- | --- |
| `deku_build_info{version}` | Daemon release version, always `1` |
| `deku_apps_total` | Apps managed by this daemon |
| `deku_apps_deployed` | Apps whose status is `deployed` |
| `deku_apps_tls_enabled` | Apps with TLS enabled |
| `deku_services_total` | Managed services |
| `deku_services_by_plugin{plugin}` | Managed services per plugin |
| `deku_containers_running` | Running containers across all apps |
| `deku_deployments_live` | Deployments currently serving traffic |
| `deku_service_backups_total` | Stored service backups |
| `deku_backup_schedules_overdue` | Backup schedules past their due time |
| `deku_alerts_active{severity}` | Active alerts per severity |

Metrics are read-only and cheap: they are counts over the database, with the exception of
`deku_backup_schedules_overdue`, which compares stored due times against the current clock.

## Alerts

The alert watcher evaluates a fixed set of rules on a timer and stores what it finds. Rules are
deliberately not user-configurable: the same checks power `deku doctor`, and keeping them in code
means an alert cannot drift from what the daemon actually verifies.

| Rule | Severity | Fires when |
| --- | --- | --- |
| `certificate_expiring` | warning | A TLS certificate is inside its renewal window |
| `certificate_expired` | critical | A TLS certificate is past `notAfter` |
| `certificate_missing` | critical | TLS is enabled but the certificate or key file is missing |
| `app_no_running_containers` | critical | An app has a deployment but no running web container |
| `backup_failed` | warning | The most recent scheduled backup for a service failed |
| `backup_schedule_overdue` | warning | A scheduled backup is past its due time |
| `object_store_unreachable` | critical | The configured object store failed a round trip |
| `disk_usage_high` | warning / critical | The filesystem holding the data directory passed a threshold |

Each alert is identified by its rule and scope (`app:<name>`, `service:<name>`, or `host`). While a
condition persists the alert is refreshed, not duplicated, so `deku alerts` stays readable during a
long outage. When the condition clears, the alert is resolved and kept as history.

### Reading alerts

```bash
deku alerts            # active alerts
deku alerts --all      # history, including resolved alerts
```

`GET /api/alerts` returns active alerts; `GET /api/alerts?include_resolved=true` returns history.
The dashboard shows active alerts on the host page.

### Notifications

Alerts use the same HTTP hook surface as deploys, so you can route them anywhere:

```toml
[[hooks]]
url = "https://alerts.example.com/deku"
secret = "shared-secret"
events = ["alert.fired", "alert.resolved"]
```

`alert.fired` and `alert.resolved` fire once per state change, not once per evaluation, so a
condition that lasts an hour produces one notification. The body carries the alert under
`detail.alert` (rule, severity, scope, subject, message, and timestamps), and every request is
signed with `X-Deku-Signature` exactly like the deploy events described in
[Lifecycle hooks](/hooks/).

## Configuration

```toml
[alerts]
enabled = true          # set false to rely on metrics alone
interval_secs = 300     # seconds between evaluations
disk_warn_percent = 85  # filesystem usage that raises a warning
disk_critical_percent = 95
```

The watcher resolves alerts through the database, so an alert raised before a restart is still
visible after it — and is resolved on the first evaluation that no longer reproduces the condition.
