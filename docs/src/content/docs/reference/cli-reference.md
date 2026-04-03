---
title: CLI Reference
description: Generated command reference for the current Deku CLI surface.
---

This page is generated from [`.codex/deku-build-plan.md`](../../../../.codex/deku-build-plan.md).
It reflects the current planned and implemented CLI surface in the repository, not a polished release contract.

## Implemented Command Summary

- `deku apps list|create|destroy|info`
- `deku config list|set|unset`
- `deku deploy run|list|rollback`
- `deku domains list|add|remove`
- `deku logs [-n] [--follow]`
- `deku ps list|scale`
- `deku ssh add|list|remove`
- `deku plugins list|install|uninstall`
- `deku objectstore setup|info|test|unset`
- `deku postgres create|destroy|link|unlink|list|info|connect|logs|backup|backups|restore`
- `deku redis create|destroy|link|unlink|list|info|connect|logs`

## Command Groups

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
- Without: tar.gz source directory, POST multipart to `/api/apps/:name/deploy/archive`
- Streams SSE deploy log to terminal
- Preferred deploy path for coding-agent workflows
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
