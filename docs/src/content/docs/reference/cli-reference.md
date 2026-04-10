---
title: CLI Reference
description: Release-facing command reference for the current Deku CLI surface.
---

Current release-facing `deku` command surface.

## Core Commands

- `deku setup`
- `deku dashboard`
- `deku uninstall`
- `deku apps list|create|destroy|info`
- `deku checks run|routing`
- `deku config list|set|unset`
- `deku deploy run|list|rollback`
- `deku domains list|add|remove`
- `deku letsencrypt enable|disable|status|config`
- `deku logs [-n] [--follow]`
- `deku ps list|scale`
- `deku ssh add|list|remove`
- `deku plugins list|install|uninstall`
- `deku objectstore setup|info|test|unset|status|link|unlink`
- `deku postgres create|destroy|link|unlink|list|info|connect|logs|backup|backups|restore`
- `deku redis create|destroy|link|unlink|list|info|connect|logs|backup|backups|restore`
- `deku mysql create|destroy|link|unlink|list|info|connect|logs`
- `deku network create|destroy|attach|detach|list|report`
- `deku storage ensure-directory|mount|unmount|list`
- `deku cron list|add|remove`
- `deku git set|report|remote add|doctor`

## Command Groups

### `deku setup`

- `setup` — interactive first-run configuration for the server

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

### `deku config`

- `config list <app>` — KEY=VALUE output
- `config set <app> KEY=VAL [KEY=VAL ...]` — batch set via per-key API writes
- `config unset <app> KEY`

### `deku deploy`

- `deploy run <app> [--path .] [--image img] [--builder b]`
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

- `logs <app> [-n 100]` — tail N lines from container logs
- `logs <app> --follow` — SSE stream of event bus

### `deku ps`

- `ps list <app>` — running process/container view
- `ps scale <app> PROC=N [PROC=N ...]`

### `deku ssh`

- `ssh add <name> <key-or-path>` — reads `.pub` file if path given
- `ssh list` — name + fingerprint table
- `ssh remove <name>`

### `deku plugins`

- `plugins list`
- `plugins install <path-to-.so>`
- `plugins uninstall <name>`

### `deku letsencrypt`

- `letsencrypt enable <app>`
- `letsencrypt disable <app>`
- `letsencrypt status <app>`
- `letsencrypt config <email>`

### `deku objectstore`

- `objectstore setup`
- `objectstore info`
- `objectstore test`
- `objectstore unset`
- `objectstore status <app>`
- `objectstore link <app> [--prefix path]`
- `objectstore unlink <app>`

### `deku postgres`

- Create, destroy, link, unlink, list, info, connect, logs, backup, backups, restore

### `deku redis`

- Create, destroy, link, unlink, list, info, connect, logs, backup, backups, restore

### `deku mysql`

- Create, destroy, link, unlink, list, info, connect, logs

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
