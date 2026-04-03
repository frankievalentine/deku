---
title: Dashboard
description: What the Deku dashboard covers today.
---

The dashboard is the web control surface served directly by `dekud`. It is a static Astro shell with live API panels and SSE-backed event/log streaming.

## Access Model

- `dekud` serves the dashboard bundle from its configured `dashboard_dir`
- the default dashboard URL is `http://127.0.0.1:2810`
- the dashboard authenticates with the same JWT token the CLI uses
- the daemon writes that token to `~/.deku/cli-token`

When you open the dashboard for the first time, paste the token into the connect screen. The browser stores it locally until you reset it from the header.

## Current Pages

- **Apps**: fleet view, app creation, app deletion, and quick links into app detail and deployment history
- **Host**: a unified admin overview for fleet status, routing/TLS health, object store state, services, access surfaces, and recent daemon events
- **App detail**: domains, ports, routing/TLS status, networks, storage mounts, cron, config vars, app metadata, safer delete workflow, desired scale, live process inventory, deployment history, and streaming logs/events
- **Routing**: routing table, Angie validation status, per-app routing health, and global Let’s Encrypt email config
- **Services**: create and inspect Postgres, Redis, and MySQL services, link them to apps, inspect logs, and manage Postgres backups/restores
- **Object Store**: configure, test, and remove the host-level S3-compatible object store used for backups
- **Deployment detail**: per-app deployment history, rollback controls, lifecycle phase summaries, and structured deploy-event console output for a selected deployment
- **SSH Keys**: add, inspect, and remove trusted public keys
- **Plugins**: inspect loaded plugins and load/unload plugin shared libraries by path

## What The Dashboard Uses Under The Hood

- `GET /api/apps`, `POST /api/apps`, `DELETE /api/apps/:name`
- `GET /api/apps/:name/deployments`
- `GET /api/apps/:name/config`, `POST /api/apps/:name/config`
- `GET /api/apps/:name/domains`, `POST /api/apps/:name/domains`
- `GET /api/apps/:name/ports`, `POST /api/apps/:name/ports`
- `GET /api/networks`, `POST /api/networks`, `DELETE /api/networks/:name`
- `GET /api/apps/:name/networks`, `POST /api/apps/:name/networks/:network`
- `GET /api/apps/:name/storage`, `POST /api/apps/:name/storage`
- `POST /api/apps/:name/storage/ensure`, `DELETE /api/apps/:name/storage/:id`
- `GET /api/apps/:name/cron`, `POST /api/apps/:name/cron`, `DELETE /api/apps/:name/cron/:id`
- `GET /api/apps/:name/ps`, `GET /api/apps/:name/scale`, `POST /api/apps/:name/scale`
- `GET /api/apps/:name/logs`
- `GET /api/apps/:name/events/stream`
- `GET /api/routing`, `GET /api/routing/status`, `GET /api/routing/status/:name`
- `POST /api/letsencrypt/enable/:app`, `POST /api/letsencrypt/disable/:app`
- `GET /api/letsencrypt/status/:app`, `GET /api/letsencrypt/config`, `POST /api/letsencrypt/config`
- `GET /api/objectstore`, `POST /api/objectstore`, `DELETE /api/objectstore`
- `POST /api/objectstore/test`
- `GET /api/postgres/services`, `POST /api/postgres/services`, `GET /api/postgres/services/:name`
- `POST /api/postgres/services/:name/link/:app`, `GET /api/postgres/services/:name/logs`
- `GET /api/postgres/services/:name/backups`, `POST /api/postgres/services/:name/backups`
- `POST /api/postgres/services/:name/restore/:backup_id`
- `GET /api/redis/services`, `POST /api/redis/services`, `GET /api/redis/services/:name`
- `POST /api/redis/services/:name/link/:app`, `GET /api/redis/services/:name/logs`
- `GET /api/mysql/services`, `POST /api/mysql/services`, `GET /api/mysql/services/:name`
- `POST /api/mysql/services/:name/link/:app`, `GET /api/mysql/services/:name/logs`
- `GET /api/ssh-keys`, `POST /api/ssh-keys`
- `GET /api/plugins`, `POST /api/plugins`

The dashboard is intentionally behind the CLI in scope, not ahead of it. If a platform surface exists only in the CLI or HTTP API, that is expected for the current milestone.

## Source Checkout Workflow

If you are running from the repo instead of a packaged install:

1. Build the dashboard in `dashboard/`.
2. Copy the built output from `crates/dekud/assets/dashboard/` into `~/.deku/dashboard/`.
3. Start `dekud`.

See [Quick Start](/quickstart/) for the exact commands.
