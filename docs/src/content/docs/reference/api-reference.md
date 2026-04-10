---
title: API Reference
description: HTTP API families used by the CLI and dashboard.
---

Current HTTP API surface used by the Deku CLI and dashboard.

## Apps and Deployments

- `GET /api/apps`
- `POST /api/apps`
- `DELETE /api/apps/:name`
- `GET /api/apps/:name`
- `GET /api/apps/:name/deployments`
- `POST /api/apps/:name/deploy`
- `POST /api/apps/:name/deploy/archive`
- `POST /api/apps/:name/rollback`

## App Configuration and Runtime

- `GET /api/apps/:name/config`
- `POST /api/apps/:name/config`
- `DELETE /api/apps/:name/config/:key`
- `GET /api/apps/:name/checks`
- `GET /api/apps/:name/domains`
- `POST /api/apps/:name/domains`
- `DELETE /api/apps/:name/domains/:domain`
- `GET /api/apps/:name/ports`
- `POST /api/apps/:name/ports`
- `DELETE /api/apps/:name/ports/:id`
- `GET /api/apps/:name/ps`
- `GET /api/apps/:name/scale`
- `POST /api/apps/:name/scale`
- `GET /api/apps/:name/logs`
- `GET /api/apps/:name/events/stream`

## Networks, Storage, and Cron

- `GET /api/networks`
- `POST /api/networks`
- `DELETE /api/networks/:name`
- `GET /api/apps/:name/networks`
- `POST /api/apps/:name/networks/:network`
- `DELETE /api/apps/:name/networks/:network`
- `GET /api/apps/:name/storage`
- `POST /api/apps/:name/storage`
- `POST /api/apps/:name/storage/ensure`
- `DELETE /api/apps/:name/storage/:id`
- `GET /api/apps/:name/cron`
- `POST /api/apps/:name/cron`
- `DELETE /api/apps/:name/cron/:id`

## Routing and TLS

- `GET /api/routing`
- `GET /api/routing/status`
- `GET /api/routing/status/:name`
- `POST /api/letsencrypt/enable/:app`
- `POST /api/letsencrypt/disable/:app`
- `GET /api/letsencrypt/status/:app`
- `GET /api/letsencrypt/config`
- `POST /api/letsencrypt/config`

## Managed Services and Object Store

- `GET /api/objectstore`
- `POST /api/objectstore`
- `DELETE /api/objectstore`
- `POST /api/objectstore/test`
- `GET /api/postgres/services`
- `POST /api/postgres/services`
- `GET /api/postgres/services/:name`
- `POST /api/postgres/services/:name/link/:app`
- `DELETE /api/postgres/services/:name/link/:app`
- `GET /api/postgres/services/:name/logs`
- `GET /api/postgres/services/:name/backups`
- `POST /api/postgres/services/:name/backups`
- `POST /api/postgres/services/:name/restore/:backup_id`
- `GET /api/redis/services`
- `POST /api/redis/services`
- `GET /api/redis/services/:name`
- `POST /api/redis/services/:name/link/:app`
- `DELETE /api/redis/services/:name/link/:app`
- `GET /api/redis/services/:name/logs`
- `GET /api/mysql/services`
- `POST /api/mysql/services`
- `GET /api/mysql/services/:name`
- `POST /api/mysql/services/:name/link/:app`
- `DELETE /api/mysql/services/:name/link/:app`
- `GET /api/mysql/services/:name/logs`

## Access and Extensions

- `GET /api/ssh-keys`
- `POST /api/ssh-keys`
- `DELETE /api/ssh-keys/:name`
- `GET /api/plugins`
- `POST /api/plugins`
- `DELETE /api/plugins/:name`

For end-user workflows, see [Get Started](/get-started/) and [Dashboard Overview](/dashboard-overview/).
