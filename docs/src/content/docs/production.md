---
title: Production checklist
description: What to check before a Deku host serves real traffic, and what changes when you upgrade an existing host.
---

A new install starts from safe defaults. This page lists what those defaults are, what you still
need to decide, and how to confirm each one. Run `deku doctor` after any change: it checks most of
the items below.

## Firewall

Open only what the host serves:

| Port | Needed for |
| --- | --- |
| `80/tcp`, `443/tcp` | App traffic through Angie. Port 80 also answers the certificate challenges, so keep it open even when everything is served over HTTPS |
| `2222/tcp` | Only if you deploy with `git push` over SSH |
| `22/tcp` | Your own SSH access |

Docker writes its own firewall rules, so a container port published on every interface is
reachable even behind UFW. Deku avoids that: web replicas publish only on `127.0.0.1`, and managed
services publish no port at all. `deku doctor` reports any Deku container that still publishes
beyond loopback.

## Certificates

New installs request a certificate for every domain you add to an app and renew it automatically.
Point the domain's DNS at the server and check progress with `deku domains list <app>`; a domain
that cannot be reached shows why. See [Automatic certificates](/certificates/).

## Dashboard access

The dashboard and HTTP API listen on `127.0.0.1:2810` by default. Reach them through an SSH tunnel:

```bash
ssh -L 2810:127.0.0.1:2810 root@YOUR_SERVER_IP
```

To reach the dashboard at a hostname instead, set `dashboard_domain` and Angie serves it over
HTTPS. See [Installation](/installation/#publish-the-dashboard-through-angie). Do not open port
`2810` to the internet: it is plain HTTP.

The dashboard token expires after 30 days. Rotate it any time with `deku dashboard reset-token`.
For CI, use an app-scoped [deploy token](/deploy-tokens/) rather than the dashboard token.

## Encryption key

`deku setup` generates `/etc/deku/encryption.key`, which seals config values and backups. Copy it
somewhere safe that is not the server and not your backup bucket. Without it, encrypted values and
backups cannot be read.

## Backups

1. Configure an object store and check it with `deku objectstore test`.
2. Schedule each service: `deku backup schedule <service> --interval-hours 24 --keep 7`.
3. Restore one backup into a scratch service to confirm the whole path works.

See [Backups](/backups/).

## Watching the host

- `deku doctor` exits non-zero when a check fails; run it from your monitoring.
- `GET /api/metrics` serves Prometheus metrics. See [Monitoring and alerts](/monitoring/).
- `deku logs <app> --follow` and the dashboard keep every deployment's logs searchable.

## Installs and upgrades

The installer checks every download against the release's `SHA256SUMS`. From v0.2.1 on, releases
are also signed, and the installer refuses one whose signature is missing or does not match the
Deku release key. Earlier releases install with a warning, checked against `SHA256SUMS` only.

Upgrading a host installed before these defaults changes some things on its own and leaves others
to you:

| What | On upgrade |
| --- | --- |
| Managed services | Moved onto the private `deku` network when `dekud` starts, on the same data volume. Linked apps get the new URL on their next deploy. |
| App replicas | Keep their old port binding until the app is redeployed. `deku doctor` lists them. |
| Dashboard binding | A config without `api_bind` keeps listening on every interface. Add `api_bind = "127.0.0.1"` and restart `dekud`. |
| Encryption key | Not generated for an existing install. Add one as described in [Backups](/backups/#encryption-at-rest). |
| File locations | An install under `~/.deku` keeps using it. |
| Certificates for app domains | Off until you run `deku acme custom-domains on`. Apps with installed certificates keep serving them. |
| Interrupted deploys | A deploy that was running when `dekud` stopped is marked failed on start, and its containers are removed. |
