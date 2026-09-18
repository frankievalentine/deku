---
title: CLI Reference
description: Release-facing command reference for the current Deku CLI surface.
---

Current release-facing `deku` command surface.

## Core Commands

- `deku setup`
- `deku version`
- `deku restart`
- `deku dashboard`
- `deku uninstall`
- `deku apps list|create|destroy|info|rename|clone`
- `deku auth enable|forward|disable|status`
- `deku backup schedule|schedules|status|unschedule`
- `deku build-host setup|info|check|init|unset`
- `deku checks run|routing`
- `deku config list|set|unset`
- `deku deploy run|list|rollback|token`
- `deku doctor`
- `deku alerts [--all]`
- `deku env list|create|remove`
- `deku domains list|add|remove`
- `deku exec <app> <command>...`
- `deku letsencrypt enable|disable|status|config`
- `deku logs [-n] [--follow] [--timeout]`
- `deku maintenance on|off|status`
- `deku ps list|scale|limits`
- `deku redirects list|add|remove`
- `deku registry setup|info|unset`
- `deku run <app> <command>...`
- `deku ssh add|list|remove`
- `deku plugins list|install|uninstall`
- `deku objectstore setup|info|test|unset|status|link|unlink`
- `deku postgres create|destroy|link|unlink|list|info|connect|logs|backup|backups|restore`
- `deku redis create|destroy|link|unlink|list|info|connect|logs|backup|backups|restore`
- `deku mysql create|destroy|link|unlink|list|info|connect|logs|backup|backups|restore`
- `deku mariadb create|destroy|link|unlink|list|info|connect|logs|backup|backups|restore`
- `deku mongodb create|destroy|link|unlink|list|info|connect|logs|backup|backups|restore`
- `deku network create|destroy|attach|detach|list|report`
- `deku storage ensure-directory|mount|unmount|list`
- `deku cron list|add|remove`
- `deku git set|report|remote add|doctor`

## Command Groups

### `deku setup`

- `setup` — interactive first-run configuration for the server

### `deku version`

- `version` — print the current tagged Deku release version
- `--version` / `-v` — print the same version string

### `deku restart`

- `restart` — restart the local `deku` systemd service on packaged Linux installs

### `deku dashboard`

- `dashboard` — print the server-reachable dashboard URL, local loopback URL, token status, SSH tunnel / firewall guidance, and reset guidance
- `dashboard --json` — print non-secret access metadata as JSON
- `dashboard reset-token [--yes]` — rotate the dashboard token and print the new value once

### `deku uninstall`

- `uninstall` — interactive packaged-install removal flow
- `uninstall --keep-data` — remove host install artifacts and keep local state and Docker volumes
- `uninstall --full-remove` — remove host install artifacts and Deku-owned persisted state
- `uninstall --yes` — skip the destructive confirmation prompt
- `uninstall --dry-run` — print the uninstall plan without making changes

### `deku apps`

- `apps list` — tabular output (name, status, created)
- `apps create <name>` — create app, print ID
- `apps destroy <name> [--force]` — confirmation prompt unless `--force`
- `apps info <name>` — JSON pretty-print
- `apps rename <name> <new-name>` — rename an app; running containers keep serving
- `apps clone <name> <new-name>` — copy an app's portable settings into a new app

`apps rename` rewrites the vhost and keeps the app running, but container names still reference the
old name until the next deploy, and any git remote that used the old name needs updating.

`apps clone` copies config vars, resource limits, redirects, and app auth. It deliberately does not
copy domains, port mappings, storage mounts, cron entries, service links, or deploy tokens, because
those would conflict with the source or duplicate work. The command prints both lists.

### `deku config`

- `config list <app>` — KEY=VALUE output, with `(global)` marking global vars
- `config set <app> KEY=VAL [KEY=VAL ...]` — batch set via per-key API writes
- `config unset <app> KEY`
- `config import <app> --file .env [--overwrite]` — import a `.env` file, skipping vars that already exist unless `--overwrite` is passed

### `deku deploy token`

- `deploy token create <app> [--name ci]` — mint a token; printed once
- `deploy token list <app>` — id, name, creation time, last use
- `deploy token revoke <app> <id>`

A deploy token can only trigger deploys for its own app. See [Deploy tokens](/deploy-tokens/).

### `deku deploy`

- `deploy run <app> [--path .] [--image img] [--builder b] [--build-host local|name]`
- `deploy token create|list|revoke` — CI credentials scoped to one app
- With `--image`: POST to `/api/apps/:name/deploy`
- Without: Tar.gz source directory, POST multipart to `/api/apps/:name/deploy/archive`
- Streams SSE deploy log to terminal
- `deploy list <app>` — tabular deployment history
- `deploy rollback <app> [--to <id>]`

### `deku domains`

- `domains list <app>`
- `domains add <app> <domain>`
- `domains remove <app> <domain>`

### `deku logs`

- `logs <app> [-n 100]` — stored lines, build and runtime, for every deployment
- `logs <app> --follow [--timeout secs]` — live log lines over SSE; `--timeout` stops after idle seconds
- `logs <app> --search <term>` — case-insensitive full-text search over stored lines
- `logs <app> --source build|runtime`, `--stream stdout|stderr`, `--level ERROR` — narrow by origin
- `logs <app> --deployment <id>`, `--environment <slug>` — narrow to one deployment or environment

Lines are stored, so a retired deployment's logs remain readable. See [Logs](/logs/).

### `deku run`

- `run <app> <command>...` — run a command in a fresh container from the app image, then remove it

### `deku exec`

- `exec <app> <command>...` — run a command inside the app's running web container

See [Runtime access](/runtime-access/) for exit-code behavior and what the container can see.

### `deku maintenance`

- `maintenance on <app> [--message text]` — serve a 503 instead of proxying
- `maintenance off <app>` — resume serving
- `maintenance status <app>`

### `deku redirects`

- `redirects list <app>`
- `redirects add <app> <source> <target> [--code 301|302|307|308]` — one exact path per entry
- `redirects remove <app> <id>`

See [Traffic control](/traffic-control/) for how both reach the proxy.

### `deku auth`

- `auth enable <app> --user user [--password pass]` — HTTP basic auth, password prompted when omitted
- `auth forward <app> --url https://auth.example/verify` — delegate to a forward-auth endpoint
- `auth disable <app>`
- `auth status <app>`

See [App authentication](/app-authentication/).

### `deku ps`

- `ps list <app>` — running process/container view
- `ps scale <app> PROC=N [PROC=N ...]`
- `ps limits <app> [--process type] [--cpu 0.5|500m] [--memory 512m|1g]` — show or set limits

See [Resource limits](/resource-limits/) for accepted formats.

### `deku ssh`

- `ssh add <name> <key-or-path>` — reads `.pub` file if path given
- `ssh list` — name + fingerprint table
- `ssh remove <name>`

### `deku plugins`

- `plugins list` — reports whether the running daemon includes the dynamic plugin runtime
- `plugins install <path-to-.so>` — fails with an explanation on builds without the runtime
- `plugins uninstall <name>`

The in-process runtime is compiled out by default. Rebuild with `--features dynamic-plugins` to
enable it, or use [lifecycle hooks](/hooks/) instead.

### `deku letsencrypt`

- `letsencrypt enable <app>`
- `letsencrypt disable <app>`
- `letsencrypt status <app> [--json]` — human-readable summary, including days until expiry
- `letsencrypt config <email>`

### `deku objectstore`

- `objectstore setup`
- `objectstore info`
- `objectstore test`
- `objectstore unset`
- `objectstore status <app>`
- `objectstore link <app> [--prefix path]`
- `objectstore unlink <app>`

### `deku build-host`

- `build-host setup [--host ssh://user@host[:port]] [--name builder] [--identity-file path] [--buildkit-host endpoint]`
- `build-host info` — configured host plus registry, password redacted
- `build-host check` — probes ssh, docker, railpack, BuildKit; exits `1` when a check fails
- `build-host init` — creates or starts the managed BuildKit container on the host
- `build-host unset`

Builds run on the build host and reach the deploy host through the registry. See
[Build server](/build-server/).

### `deku registry`

- `registry setup [--server ghcr.io/acme] [--username user] [--password secret] [--namespace deku]`
- `registry info` — password redacted
- `registry unset`

### `deku postgres`

- Create, destroy, link, unlink, list, info, connect, logs, backup, backups, restore

### `deku redis`

- Create, destroy, link, unlink, list, info, connect, logs, backup, backups, restore

### `deku mysql`

- Create, destroy, link, unlink, list, info, connect, logs, backup, backups, restore

### `deku mariadb`

- Create, destroy, link, unlink, list, info, connect, logs, backup, backups, restore
- Uses the same command shape as `mysql`, with `MARIADB_URL` as the injected env key

### `deku mongodb`

- Create, destroy, link, unlink, list, info, connect, logs, backup, backups, restore
- Injects `MONGODB_URL` and authenticates against the `admin` database

### Generic service routes

Postgres, Redis, and MySQL have per-type routes (`/api/postgres/services/...`). MariaDB and MongoDB
use the generic family, and the per-type routes remain as aliases:

- `GET|POST /api/services/{type}`
- `GET|DELETE /api/services/{type}/{name}`
- `POST|DELETE /api/services/{type}/{name}/link/{app}`
- `GET /api/services/{type}/{name}/logs`
- `GET|POST /api/services/{type}/{name}/backups`
- `POST /api/services/{type}/{name}/restore/{backup_id}`

`{type}` is one of `postgres`, `redis`, `mysql`, `mariadb`, or `mongodb`.

### `deku backup`

- `backup schedule <service> [--interval-hours 24] [--keep 7]`
- `backup schedules` — every schedule with last run status and next run time
- `backup status <service>`
- `backup unschedule <service>`

See [Backups](/backups/).

### `deku network`

- `network create <name>`
- `network destroy <name>`
- `network attach <app> <network>`
- `network detach <app> <network>`
- `network list`
- `network report <app>`

### `deku storage`

- `storage ensure-directory <app> <path>`
- `storage mount <app> <host-path> <container-path>`
- `storage unmount <app> <id>`
- `storage list <app>`

### `deku cron`

- `cron list <app>`
- `cron add <app> <schedule> <command>`
- `cron remove <app> <id>`

### `deku checks`

- `checks run <app> [--path /health] [--timeout 5]`
- `checks routing [app]`

### `deku git`

- `git set <app> <key> <value>`
- `git report <app>`
- `git remote add <app>`
- `git doctor`

### `deku env`

Every app has a `production` environment, which is what `deku deploy run <app>` targets. Additional
environments are named deployment targets for the same app.

- `env list <app>` — slug, display name, tracked branch, and whether it is production
- `env create <app> <name> [--slug <slug>] [--branch <ref>]` — create one; the slug is derived from
  the name unless given, and an explicit slug must be lowercase letters, digits, and `-`
- `env remove <app> <slug>` — remove one; production cannot be removed

`--branch` records the git ref an environment tracks. Nothing auto-deploys on push yet, so it is
metadata used for display and for choosing a target.

### `deku alerts`

- `alerts` — active alerts (severity, rule, subject, first seen, message)
- `alerts --all` — alert history, including resolved alerts

Deku does not renew or issue certificates; the alert watcher reports expiry so an operator or
external automation can act.

### `deku doctor`

- `doctor` — run host and daemon checks; exits `1` when any check has failed

See [Diagnostics](/diagnostics/).
