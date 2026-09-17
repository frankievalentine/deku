---
title: Diagnostics
description: Check host and daemon health, and stop a log stream once it goes quiet.
---

## Check host health

```bash
deku doctor
```

`deku doctor` runs a set of checks and prints one row per check:

```
Status: ok
CHECK              STATE  DETAIL
------------------------------------------------------------------------
daemon             ok     dekud v0.1.12
database           ok     query succeeded
docker             ok     daemon reachable
angie_config_dir   ok     /etc/angie/conf.d/deku
object_store       warn   not configured; backups unavailable
inventory          ok     3 apps, 1 services
```

States are `ok`, `warn`, and `fail`. A `warn` means a feature is unavailable while the host is still serving; a `fail` means something is broken. `deku doctor` exits `1` when any check has failed, so you can use it as a health gate in a script or monitoring probe.

The checks cover the daemon version, the database, the Docker daemon, the Angie config directory, the object store, and the app and service inventory.

## Stop a log stream after it goes quiet

`deku logs --follow` streams until you interrupt it. Add `--timeout` to stop after a period with no output:

```bash
deku logs my-app --follow --timeout 30
```

The timer resets on every line, so the command exits 30 seconds after the app stops logging. Use it to capture a deploy or a boot sequence without leaving a process waiting for output that never comes.

## Certificate expiry

Deku does not issue or renew certificates. `deku letsencrypt enable` requires the certificate and
key files to exist already, so renewal is an operator or external-automation concern. What Deku
does provide is visibility:

- `deku letsencrypt status <app>` prints the expiry date and days remaining, and warns when the
  certificate is expiring (inside 14 days), expired, missing, or unreadable. `--json` returns the
  raw payload, including `lifecycle`, `expires_at`, and `days_remaining`.
- `deku doctor` reports a `tls_certificates` check across every TLS-enabled app and fails when any
  certificate is missing or expired.
- The dashboard's routing panel shows how long the certificate is valid for, with a warning callout.
- The alert watcher raises `certificate_expiring`, `certificate_expired`, and `certificate_missing`
  alerts, delivered over the hook surface like any other event.

Certificates live at `/etc/angie/ssl/deku_<app>.crt` and `/etc/angie/ssl/deku_<app>.key`.
