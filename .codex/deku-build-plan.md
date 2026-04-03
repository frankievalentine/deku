# Deku — Build Plan

> A modern, lightweight self-hosted PaaS inspired by Dokku.
> Built with Rust, Angie, and Astro. MIT licensed.

---

## Progress Legend

- [x] Complete
- [-] Partial / In progress
- [ ] Not started

---

## Milestone Status Summary

| Milestone | Status |
|---|---|
| 1 — Core Daemon & SQLite Schema | [x] Complete |
| 2 — Angie Integration & Routing | [x] Complete |
| 3 — Deploy Pipeline & Build System | [x] Complete |
| 4 — CLI & Plugin System | [-] In progress (~70%) |
| 5 — Core Plugins | [-] In progress (~25%) |
| 6 — Astro Dashboard | [-] In progress (~70%) |
| 7 — Starlight Documentation | [-] In progress (~70%) |
| 8 — Bootstrap & Distribution | [-] In progress (~75%) |

---

## Table of Contents

1. [Project Overview](#1-project-overview)
2. [Architecture Overview](#2-architecture-overview)
3. [Repository Structure](#3-repository-structure)
4. [Full Technology Stack](#4-full-technology-stack)
5. [Complete Feature Surface](#5-complete-feature-surface)
6. [Milestone 1 — Core Daemon & SQLite Schema](#6-milestone-1--core-daemon--sqlite-schema)
7. [Milestone 2 — Angie Integration & Routing](#7-milestone-2--angie-integration--routing)
8. [Milestone 3 — Deploy Pipeline & Build System](#8-milestone-3--deploy-pipeline--build-system)
9. [Milestone 4 — CLI & Plugin System](#9-milestone-4--cli--plugin-system)
10. [Milestone 5 — Core Plugins](#10-milestone-5--core-plugins)
11. [Milestone 6 — Astro Dashboard](#11-milestone-6--astro-dashboard)
12. [Milestone 7 — Starlight Documentation](#12-milestone-7--starlight-documentation)
13. [Milestone 8 — Bootstrap & Distribution](#13-milestone-8--bootstrap--distribution)
14. [System Requirements](#14-system-requirements)
15. [Install Story](#15-install-story)
16. [Command Reference](#16-command-reference)
17. [Plugin Architecture](#17-plugin-architecture)
18. [deku.toml Reference](#18-dekutoml-reference)

---

## 1. Project Overview

**Deku** is a single-server PaaS platform that lets developers deploy applications
via `git push` or direct Docker image/archive deploys. It advances on Dokku's
foundation with a compiled Rust daemon, Angie as the reverse proxy (replacing
nginx), a web dashboard served from the daemon itself, and a modern CLI powered
by cliclack.

**Core principles:**
- Single-binary Rust control plane after install; system integrations remain Docker, Angie, and Git
- CLI-first, dashboard-second. Everything the dashboard can do, the CLI can do
- Lightweight by default: daemon idles at ~5–15 MB RSS
- Docker-native: container lifecycle managed entirely via the Docker Engine API
- Opinionated integrated platform first, with extension hooks and first-party plugin-style modules
- MIT licensed throughout

**Must-Ship Platform Core:**
- Remote deploy to server over SSH / `git push`
- Direct deploy from source archive or pre-built image
- Domains, routing, and TLS
- Persistent app storage
- Managed Postgres and Redis
- Bootstrap / install / release packaging
- Dashboard and docs only to the extent they support the core platform

---

## 2. Architecture Overview

```
Developer Machine
  │
  │  git push deku main  (SSH → port 22)
  │  deku config:set ...  (SSH remote command)
  │
  ▼
┌─────────────────────────────────────────────────────┐
│  Deku Server                                        │
│                                                     │
│  ┌──────────┐   Unix socket   ┌─────────────────┐  │
│  │  deku    │◄───────────────►│    dekud        │  │
│  │  (CLI)   │                 │  (daemon)       │  │
│  └──────────┘                 │                 │  │
│                               │  - Axum API     │  │
│  ┌──────────┐   WebSocket     │  - Deploy FSM   │  │
│  │ Dashboard│◄───────────────►│  - Plugin bus   │  │
│  │ (Astro)  │   REST          │  - Event log    │  │
│  └──────────┘                 │  - SSH server   │  │
│                               └────────┬────────┘  │
│                                        │            │
│                               ┌────────▼────────┐  │
│                               │   SQLite DB     │  │
│                               │  ~/.deku/deku.db│  │
│                               └────────┬────────┘  │
│                                        │            │
│                               ┌────────▼────────┐  │
│                               │  Docker Engine  │  │
│                               │  (via bollard)  │  │
│                               └────────┬────────┘  │
│                                        │            │
│  ┌─────────────────────────────────────▼─────────┐ │
│  │                Angie                          │ │
│  │  - Reverse proxy (HTTP/HTTPS/HTTP3)           │ │
│  │  - TLS termination via native ACME            │ │
│  │  - vhost routing per app                      │ │
│  │  - Dynamic upstream updates via REST API      │ │
│  │  - Prometheus metrics                         │ │
│  └───────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────┘
         │                    │
    App traffic           App traffic
  app1.domain.tld      app2.domain.tld
```

---

## 3. Repository Structure

```
deku/
├── Cargo.toml                  # Workspace root
├── Cargo.lock
├── rust-toolchain.toml
│
├── crates/
│   ├── dekud/                  # Main daemon binary
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── api/            # Axum HTTP + SSE server
│   │   │   ├── deploy/         # Deploy pipeline state machine
│   │   │   ├── build/          # Builder trait + implementations
│   │   │   ├── proxy/          # Angie config writer + reloader
│   │   │   ├── container/      # bollard wrapper
│   │   │   ├── ssh/            # Embedded SSH server + key mgmt
│   │   │   ├── events/         # Structured event log (broadcast + DB)
│   │   │   ├── plugins/        # Plugin loader + hook dispatcher
│   │   │   └── db/             # SQLite schema + queries (sqlx)
│   │   └── migrations/
│   │       ├── 0001_initial_schema.sql
│   │       ├── 0002_add_containers.sql
│   │       └── 0003_add_ssh_key_fingerprint.sql
│   │
│   ├── deku/                   # CLI binary
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── commands/       # apps, config, deploy, domains, logs, ps, ssh, plugins
│   │   │   ├── client/         # Daemon API client (reqwest + JWT + SSE)
│   │   │   └── prompt/         # cliclack helpers (stub)
│   │   └── Cargo.toml
│   │
│   ├── deku-core/              # Shared types, traits, errors
│   └── deku-plugin-sdk/        # SDK for plugin authors
│
├── plugins/                    # First-party plugin crates (currently mostly hook stubs)
│   ├── deku-plugin-postgres/
│   ├── deku-plugin-redis/
│   ├── deku-plugin-mysql/
│   ├── deku-plugin-letsencrypt/
│   ├── deku-plugin-checks/
│   ├── deku-plugin-storage/
│   ├── deku-plugin-network/
│   ├── deku-plugin-cron/
│   ├── deku-plugin-git/
│   └── deku-plugin-domains/
│
├── dashboard/                  # Astro + React dashboard (not yet built)
├── docs/                       # Starlight documentation (not yet built)
└── scripts/                    # install.sh, build-release.sh (not yet built)
```

---

## 4. Full Technology Stack

| Layer | Crate / Tool | Purpose |
|---|---|---|
| Core daemon | `tokio` | Async runtime |
| Core daemon | `axum` | HTTP API + SSE server |
| Core daemon | `bollard` 0.20 | Docker Engine API client |
| Core daemon | `sqlx` + SQLite | State persistence |
| Core daemon | `russh` 0.59 | Embedded SSH server |
| Core daemon | `serde` / `serde_json` | Serialisation |
| Core daemon | `tracing` + `tracing-subscriber` | Structured logging |
| Core daemon | `jsonwebtoken` | JWT auth for TCP endpoint |
| Core daemon | `handlebars` | Angie config template rendering |
| CLI | `clap` | Argument parsing + subcommand routing |
| CLI | `cliclack` | Interactive prompts, spinners, progress |
| CLI | `reqwest` (multipart, stream, json) | Daemon API client |
| Proxy | Angie | Reverse proxy, TLS, ACME, HTTP/3 |
| Build (primary) | Dockerfile via `bollard` | Direct Docker build |
| Build (fallback) | Nixpacks subprocess | Auto-detect language, generate image |
| Build (alt) | CNB `pack` subprocess | Cloud Native Buildpacks |
| Build (multi) | Docker Compose parser (`serde_yaml`) | Multi-service topology |
| Build (bypass) | Pre-built image pull | Skip build entirely |
| Dashboard | Astro + React | Static SPA + islands |
| Documentation | Astro Starlight | Docs site |
| Plugin system | Rust trait + `libloading` | Dynamic `.so`/`.dylib` plugin loading |
| State | SQLite via `sqlx` (offline mode) | Single-file database |
| Distribution | GitHub Releases | Pre-built binaries per arch |
| Install | `install.sh` bash script | Bootstrap on fresh server |
| CI | GitHub Actions | Build, test, release |

---

## 5. Complete Feature Surface

| Domain | Features |
|---|---|
| **Apps** | create, destroy, rename, clone, list, lock, unlock |
| **Deploy** | git push over SSH, dockerfile, image, archive, nixpacks auto-detect |
| **Build** | deku.toml config, Procfile, EXPOSE auto-detect, release phase |
| **Config** | set, get, unset, show, clear, export, bundle — global + per-app |
| **Process** | start, stop, restart, rebuild, scale, run (one-off), restart policy |
| **Zero downtime** | health probes, container retirement grace period (60s) |
| **Domains** | add, remove, set, set-global, clear |
| **Ports** | add, remove, list, EXPOSE auto-detection |
| **Network** | create, destroy, attach phases, inter-app DNS aliases |
| **Proxy** | Angie vhost routing, HTTP/3, native ACME/TLS |
| **Storage** | mount, unmount, list, ensure-directory, S3-compatible object-store config |
| **Resources** | CPU / memory limits per app and per process type |
| **SSH keys** | add, remove, list (fingerprint-indexed) |
| **Logs** | stream, follow (SSE), tail N lines |
| **Events** | structured event log, per-app history, SSE stream |
| **Plugins** | install, uninstall, list, hook dispatch via libloading |

---

## 6. Milestone 1 — Core Daemon & SQLite Schema [x] COMPLETE

### What was built

#### [x] 1.1 Workspace & Crate Scaffolding
- Cargo workspace: `deku`, `dekud`, `deku-core`, `deku-plugin-sdk`, 10 plugin crates
- `rust-toolchain.toml` pinned to stable
- Workspace-level lints: `unsafe_code = "deny"`, clippy pedantic
- GitHub Actions CI: fmt, clippy, test, sqlx offline check
- `SQLX_OFFLINE=true` build via `.sqlx/` query cache

#### [x] 1.2 SQLite Schema (sqlx migrations)
All 12 tables defined in `migrations/0001_initial_schema.sql`:
- `apps`, `deployments`, `config_vars`, `domains`, `port_mappings`
- `storage_mounts`, `resource_limits`, `ssh_keys`, `events`
- `process_scale`, `docker_options`, `networks`, `app_networks`

Additional migrations:
- `0002_add_containers.sql` — `containers` table for tracking live container state
- `0003_add_ssh_key_fingerprint.sql` — `fingerprint` column on `ssh_keys`

#### [x] 1.3 Axum API Server (`crates/dekud/src/api/mod.rs`)
- Unix socket (`~/.deku/deku.sock`) — trusted, no auth
- TCP (`0.0.0.0:2810`) — JWT Bearer auth middleware
- Full route table: apps CRUD, events, domains, ports, deployments, deploy/rollback, logs, config vars, process scale, routing
- SSH keys endpoints: `GET/POST /api/ssh-keys`, `DELETE /api/ssh-keys/:name`
- Plugins endpoints: `GET/POST /api/plugins`, `DELETE /api/plugins/:name`
- JWT token written to `~/.deku/cli-token` on daemon start

#### [x] 1.4 Event Log (`crates/dekud/src/events/mod.rs`)
- `EventBus` wraps `tokio::sync::broadcast` channel + `SqlitePool`
- All events persisted asynchronously via `tokio::spawn`
- `GET /api/events` with `?app=` and `?since=` filters
- `GET /api/apps/:name/events/stream` — Server-Sent Events (axum SSE + BroadcastStream)

#### [x] 1.5 Configuration File (`crates/dekud/src/config.rs`)
- `~/.deku/config.toml` read/written on daemon startup
- Fields: `data_dir`, `dashboard_port`, `api_port`, `ssh_port`, `angie_conf_dir`, `auth_secret`
- `auth_secret` auto-generated (two concatenated UUIDs) if missing; config saved back to disk

#### [x] 1.6 Structured Logging
- `tracing` with JSON formatter → `~/.deku/logs/dekud.log`
- Log rotation via `tracing-appender`
- Rolling daily logs

#### [x] Full DB query layer (`crates/dekud/src/db/queries.rs`)
All queries implemented: apps CRUD, events, deployments, config_vars, domains, port_mappings, storage_mounts, resource_limits, containers, process_scale, ssh_keys (add/list/find_by_fingerprint/remove)

---

## 7. Milestone 2 — Angie Integration & Routing [x] COMPLETE

### What was built

#### [x] 2.1 Angie Config Writer (`crates/dekud/src/proxy/writer.rs`)
- `write_app_config(conf_dir, app_name, domains, upstreams, tls)` renders Handlebars templates
- `remove_app_config(conf_dir, app_name)` removes config file
- Templates embedded at compile time via `include_str!`

#### [x] 2.2 Angie Config Templates
- `proxy/templates/http_app.conf.hbs` — HTTP vhost with upstream block, proxy headers, WebSocket upgrade
- `proxy/templates/https_app.conf.hbs` — HTTPS variant

#### [x] 2.3 Angie Reloader (`crates/dekud/src/proxy/reloader.rs`)
- Reads `/run/angie/angie.pid`, sends `kill -HUP <pid>` via `tokio::process::Command`
- No-op if pid file absent (dev/CI environments)
- No `unsafe` — uses subprocess instead of `libc::kill`
- Validates config before reload, and app config updates now restore the previous fragment if validation/reload fails

#### [x] 2.4 Routing Table API
- `GET /api/routing` — all app → upstream mappings
- `POST /api/routing/:name` — update upstream

#### [x] 2.5 Domain Management API
- `GET/POST /api/apps/:name/domains`
- `DELETE /api/apps/:name/domains/:domain`

#### [x] 2.6 Port Management API
- `GET/POST /api/apps/:name/ports`
- `DELETE /api/apps/:name/ports/:id`

#### [-] 2.7 Routing / TLS Reconciliation Hardening
- Domain and port mutations now roll back their DB-side change if Angie reconciliation fails
- Deploy-time proxy updates now preserve the app's current TLS mode instead of forcing HTTP
- `GET /api/letsencrypt/status/:app` exposes Deku-side TLS/cert file visibility, and `deku letsencrypt status <app>` surfaces it from the CLI
- `GET /api/routing/status` and `deku checks routing [app]` provide a consolidated routing/TLS/operator view across domains, upstreams, proxy fragments, and cert files

---

## 8. Milestone 3 — Deploy Pipeline & Build System [x] COMPLETE

### What was built

#### [x] 3.1 Builder Trait (`crates/dekud/src/build/mod.rs`)
```rust
pub trait Builder: Send + Sync {
    fn name(&self) -> &'static str;
    fn detect(&self, source: &Path) -> bool;
    async fn build(&self, ctx: &BuildContext, deku_toml: Option<&DekuToml>,
                   docker: &Docker, events: &EventSender) -> Result<BuiltImage, DekuError>;
}
```
- `select_builder(source, forced)`: ComposeBuilder → DockerfileBuilder → PackBuilder → NixpacksBuilder

#### [x] 3.2 DockerfileBuilder
- bollard 0.20 `build_image` with tar context (`create_tar_gz`)
- Streams build log lines to EventBus in real time
- Extracts `EXPOSE` ports from image metadata post-build
- Extracts Procfile from image filesystem

#### [x] 3.3 Docker Compose Builder
- Parses `docker-compose.yml` via `serde_yaml`
- Finds `web` service or `x-deku-web: true` annotated service
- Builds image via bollard or re-tags existing `image:` reference

#### [x] 3.4 NixpacksBuilder
- Subprocess `nixpacks build <dir> --name <tag>`
- Streams stdout to EventBus

#### [x] 3.5 PackBuilder
- Subprocess `pack build <tag> --path <dir>`

#### [x] 3.6 Pre-built Image deploy
- Pull via bollard `CreateImageOptionsBuilder`
- Re-tag to `deku/<app-name>:latest`

#### [x] 3.7 Archive Deploy
- Download (HTTP/S) or read from disk
- Extract tar.gz to tempdir, run builder detection

#### [x] 3.8 Deploy Pipeline FSM (`crates/dekud/src/deploy/mod.rs`)
Full 17-step pipeline:
1. Create deployment record
2. Emit `deploy.started`
3. Build phase (calls builder)
4. Update deployment status → `built`
5. Parse Procfile / default to `web`
6. Run release phase (ephemeral container) if `release:` entry
7. Update status → `deploying`
8. Collect previous containers for retirement
9. Start new containers per process type × scale
   - Inject config vars as env
   - Apply storage mounts
   - Apply resource limits (memory bytes + CPU quota)
   - Named `deku.<app>.<proc>.<deploy_id[..8]>-<i>`
10. Update status → `health_checking`
11. Wait `CHECKS_WAIT` seconds
12. HTTP health check (`reqwest`) — retry up to 5×
13. On failure: stop/remove new containers, mark `failed`, return error
14. Update port mappings in DB
15. Write Angie vhost config + reload
16. Emit `deploy.live` with URL
17. Mark deployment `live`
18. Background task: wait 60s, stop/remove old containers

#### [x] 3.9 Rollback
- Re-deploys previous deployment's image tag
- Marks current deployment as `rolled_back`

#### [x] 3.10 Release Phase (`run_release_phase`)
- bollard 0.20 API: `ContainerCreateBody`, `CreateContainerOptionsBuilder`, `LogsOptionsBuilder`, `WaitContainerOptionsBuilder`, `RemoveContainerOptionsBuilder`
- Streams logs to EventBus, checks exit code

#### [x] 3.11 bollard 0.20 Migration (all breaking changes resolved)
- All `*Options` structs → `bollard::query_parameters::*Builder` pattern
- `Config::<String>` → `bollard::models::ContainerCreateBody`
- `bollard::body_full` used directly at call sites (BodyType is `pub(crate)`)
- `BuildInfo.error_detail.and_then(|e| e.message)` for build errors
- `BuildInfo.aux.and_then(|a| a.id)` for image ID extraction

#### [x] sqlx offline mode
- `.sqlx/` query cache generated via `cargo sqlx prepare --workspace`
- All compile-time queries verified against `/tmp/deku_prepare.db`
- CI builds with `SQLX_OFFLINE=true`

---

## 9. Milestone 4 — CLI & Plugin System [-] IN PROGRESS

### Completed

#### [x] 4.1 CLI Binary (`crates/deku/src/`)
All command groups implemented with full subcommand routing:

**`deku apps`** (`commands/apps.rs`)
- `apps list` — tabular output (name, status, created)
- `apps create <name>` — create app, print ID
- `apps destroy <name> [--force]` — confirmation prompt unless `--force`
- `apps info <name>` — JSON pretty-print

**`deku config`** (`commands/config.rs`)
- `config list <app>` — KEY=VALUE output
- `config set <app> KEY=VAL [KEY=VAL ...]` — batch set via per-key API writes
- `config unset <app> KEY`

**`deku deploy`** (`commands/deploy.rs`)
- `deploy run <app> [--path .] [--image img] [--builder b]`
  - With `--image`: POST to `/api/apps/:name/deploy`
  - Without: tar.gz source directory, POST multipart to `/api/apps/:name/deploy/archive`
  - Streams SSE deploy log to terminal
  - Preferred deploy path for coding-agent workflows
- `deploy list <app>` — tabular deployment history
- `deploy rollback <app> [--to <id>]`

**`deku domains`** (`commands/domains.rs`)
- `domains list <app>`
- `domains add <app> <domain>`
- `domains remove <app> <domain>`

**`deku logs`** (`commands/logs.rs`)
- `logs <app> [-n 100]` — tail N lines from container logs
- `logs <app> --follow` — SSE stream of event bus

**`deku ps`** (`commands/ps.rs`)
- `ps list <app>` — running process/container view
- `ps scale <app> PROC=N [PROC=N ...]`

**`deku ssh`** (`commands/ssh.rs`)
- `ssh add <name> <key-or-path>` — reads `.pub` file if path given
- `ssh list` — name + fingerprint table
- `ssh remove <name>`

**`deku plugins`** (`commands/plugins.rs`)
- `plugins list`
- `plugins install <path-to-.so>`
- `plugins uninstall <name>`

**`client/mod.rs`** — Full `DekuClient`:
- JWT token loaded from `~/.deku/cli-token`
- `get`, `post`, `patch`, `delete`, `post_archive` (multipart)
- `stream_sse` — async SSE consumer

#### [x] 4.3 Plugin Loader (`crates/dekud/src/plugins/mod.rs`)
- `PluginRegistry` with `RwLock<HashMap<String, LoadedPlugin>>`
- `load_all(dir)` — scans `~/.deku/plugins/` for `.so`/`.dylib`
- `load_plugin(path)` — `libloading::Library::new` + `deku_plugin_create` symbol
- `unload_plugin(name)`
- `list_plugins()` → `Vec<(name, version, path)>`
- Hook dispatchers: `run_pre_build`, `run_post_build`, `run_pre_deploy`, `run_post_deploy`, `run_app_create`, `run_app_destroy`
- `#![allow(unsafe_code)]` scoped to this module (libloading requires unsafe)

#### [x] 4.4 SSH Server (`crates/dekud/src/ssh/mod.rs`)
- `russh` 0.59 embedded SSH server on configurable port
- Host key: Ed25519, auto-generated to `~/.deku/ssh_host_ed25519_key`, perms 0o600
- Auth: `auth_publickey` validates SHA-256 fingerprint against `ssh_keys` DB table
- Handles `git-receive-pack` exec command:
  - Pipes channel stdin/stdout/stderr ↔ subprocess
  - On exit 0: `git checkout HEAD` to tempdir → `run_deploy()`
  - Bare git repos at `~/.deku/git-repos/<app-name>.git`

#### Product Direction
- Deku should be treated as an integrated single-server PaaS, not a pure plugin host
- SSH deploy is part of the must-ship core experience, not an optional edge capability
- The dynamic plugin runtime remains valuable for hooks and future third-party extensions, but first-party platform features should ship through the clearest reliable implementation path

### Remaining

#### [ ] 4.2 Interactive Setup (`deku setup`)
- cliclack walkthrough for first-run configuration
- Writes `~/.deku/config.toml`, starts systemd unit

#### [ ] 4.5 Hardening / parity cleanup
- Audit CLI / daemon / dashboard parity for implemented routes
- Reduce stale plan drift as milestone items are completed
- Run full `cargo check` and `cargo clippy` pass with SSH/plugin code paths

#### [ ] 4.6 Git remote UX
- Remote add / doctor guidance for `git push deku main`
- Better SSH deploy diagnostics for missing key / unknown app / deploy failure cases

#### [x] 4.7 Agent Integration v1
- Repo-root `AGENTS.md` defines the supported coding-agent contract
- Preferred agent path: `deku` CLI first, daemon HTTP API second, SSH `git push` supported but secondary
- Preferred deploy path for agents: archive/image deploy over `git push`
- MCP explicitly deferred from v1; documented as a follow-on interface after CLI/API workflows stabilize

---

## 10. Milestone 5 — Core Plugins [-] IN PROGRESS

### Audit Notes

- [x] Current first-party "plugin" UX is split across two layers:
  - Dynamic `cdylib` plugins load through `PluginRegistry`
  - High-value managed services (`postgres`, `redis`, `mysql`, etc.) are currently implemented as built-in daemon service modules plus CLI commands
- [x] The plugin loader foundation works for load/list/unload, but the lifecycle hook dispatcher is not yet wired into app create/destroy or the deploy pipeline, so first-party plugin crates are still mostly hook-only stubs today
- [x] Milestone 5 work should therefore prioritise finishing the built-in first-party service flows end-to-end first, then either bridging them into the dynamic plugin runtime or documenting the split explicitly
- [x] Milestone 5 should stay focused on the must-ship platform core before expanding to lower-value plugin surface area

### Current Progress

#### Built-in Object Store (S3 / R2) [-]
- [x] `deku objectstore setup|info|test|unset`
- [x] Daemon API endpoints for get/set/unset/test
- [x] `deku setup` can write object-store config during first-run bootstrap
- [x] S3-compatible signed write/read/delete connectivity test path
- [x] Use object store for Postgres backup / restore with persisted backup metadata
- [ ] Extend object store use to Redis snapshots, restore artifacts, and release artifact retention
- [ ] App-level credential/linking workflow

#### `@deku/plugin-postgres` [-]
- [x] `deku postgres create|destroy|link|unlink|list|info|connect|logs|backup|backups|restore`
- [x] Daemon API endpoints for create/destroy/link/unlink/list/info/logs
- [x] Persistent named Docker volume for data durability
- [x] Readiness wait before service record is persisted
- [x] Rich service info payload: connection URL, host/port, username, volume, linked apps
- [x] Object-store-backed backup and restore workflow with persisted backup metadata
- [ ] Hook integration for `on_app_destroy` / `on_post_deploy`

#### `@deku/plugin-redis` [-]
- [x] `deku redis create|destroy|link|unlink|list|info|connect|logs`
- [x] Daemon API endpoints for create/destroy/link/unlink/list/info/logs
- [x] Password-protected Redis service with authenticated `REDIS_URL`
- [x] Persistent named Docker volume with AOF enabled
- [x] Readiness wait before service record is persisted
- [x] Rich service info payload: connection URL, host/port, volume, linked apps
- [ ] Snapshot / restore workflow
- [ ] Hook integration for `on_app_destroy` / `on_post_deploy`

### `@deku/plugin-postgres`
- `postgres:create <name>` — start Postgres container via bollard
- `postgres:destroy <name>`
- `postgres:link <service> <app>` — injects `DATABASE_URL`
- `postgres:unlink`, `postgres:list`, `postgres:info`, `postgres:connect`
- `postgres:backup`, `postgres:restore`, `postgres:logs`
- Hooks: `on_app_destroy` (unlink), `on_post_deploy` (verify connection)

### `@deku/plugin-redis`
- Same pattern: create, destroy, link, unlink, list, info, logs
- Injects `REDIS_URL`

### `@deku/plugin-mysql`
- Same pattern. Injects `DATABASE_URL` (MySQL DSN)

### `@deku/plugin-letsencrypt`
- Wraps Angie's native ACME support
- `letsencrypt:enable <app>`, `letsencrypt:disable <app>`
- `letsencrypt:set --global email <email>`
- `letsencrypt:auto-renew`
- Hook: `on_post_deploy` — check domain resolves before enabling

### `@deku/plugin-checks`
- Reads `CHECKS` file from deployed container or `app.json` healthchecks
- `checks:run <app>` — run checks against current live container
- Integrates with deploy pipeline health check phase

### `@deku/plugin-storage`
- `storage:ensure-directory`, `storage:mount`, `storage:unmount`, `storage:list`
- Hook: `on_pre_deploy` — validate mount paths exist

### `@deku/plugin-network`
- `network:create`, `network:destroy`, `network:set`, `network:list`, `network:report`
- Hook: `on_post_deploy` — attach containers to configured networks

### `@deku/plugin-cron`
- Reads cron config from `deku.toml`
- Manages cron containers via bollard
- `cron:list <app>`, `cron:report <app>`

### `@deku/plugin-git`
- Manages bare repos on server per app
- `git:set <app> deploy-branch <branch>`
- `git:sync <app> <remote-url>`
- `git:from-image`, `git:from-archive`, `git:report`

### `@deku/plugin-domains`
- Extends domain management with global domain patterns
- Integrates with letsencrypt plugin

### Priority Order
1. SSH deploy and Git remote ergonomics
2. Domains / routing / TLS
3. Postgres and Redis
4. Persistent storage
5. Bootstrap / install / release packaging
6. Built-in object storage for backups / artifacts
7. Secondary services: MySQL, cron, network, checks, advanced git controls

---

## 11. Milestone 6 — Astro Dashboard [-] IN PROGRESS

**Goal:** Real-time web dashboard served by the daemon. Shows app list, deployment
history, live log streaming, config vars, process status.

### Tasks
- [x] Astro + React SPA in `dashboard/`
- [x] Pages: Apps list, App detail, Deploy detail, Logs (SSE), Config, Domains, Plugins
- [x] API client using `fetch` + EventSource for SSE log streaming
- [-] Build output served by `dekud` static fallback; asset staging/embedding still needs finalisation
- [x] Dashboard URL printed on every deploy
- [x] Agent Operations docs page and repo-root `AGENTS.md`

---

## 12. Milestone 7 — Starlight Documentation [-] IN PROGRESS

**Goal:** Full documentation site co-located in repo, deployable to any static host.

### Tasks
- [x] Astro Starlight in `docs/`
- [x] Sections: Getting Started, CLI Reference, Plugin API, deku.toml, Self-hosting, Contributing
- [x] Auto-generated command reference from build plan

---

## 13. Milestone 8 — Bootstrap & Distribution [-] IN PROGRESS

**Goal:** One-line install on a fresh Ubuntu/Debian server. Pre-built binaries
for amd64 and arm64 published to GitHub Releases.

### Tasks
- `scripts/install.sh`:
  - Detect arch, download correct binary from GitHub Releases
  - Install Angie (apt)
  - Write systemd unit `deku.service`
  - Run `deku setup` non-interactively with sensible defaults
  - Verify artifact checksums when `SHA256SUMS` is published
  - Replace dashboard assets safely on upgrade and restart `dekud` so repeated installs actually activate the new release
  - Verify Angie, `dekud`, the API health endpoint, and the unix socket before reporting success
- `scripts/test-install-smoke.sh`: mock-based install + upgrade smoke coverage without requiring a real systemd/Angie host
- Cross-compile musl static binaries in CI
- `scripts/build-release.sh` for local release builds + checksum generation
- GitHub Actions `release.yml`: tag → run the same packaging path used locally → upload artifacts + checksums
- GitHub Actions `ci.yml`: run installer smoke coverage alongside Rust/dashboard/docs checks
- systemd unit file at `scripts/package/deku.service`
- Base Angie config fragment at `scripts/package/angie-deku.conf`

---

## 14. System Requirements

| Component | Minimum |
|---|---|
| OS | Ubuntu 22.04+ / Debian 11+ |
| CPU | 1 vCPU |
| RAM | 512 MB |
| Disk | 10 GB |
| Docker | 24.0+ |
| Angie | Latest stable |
| Git | 2.x (for `git push` deploys) |

Optional: `nixpacks` or `pack` CLI for buildpack-based deploys.

---

## 15. Install Story

```bash
curl -fsSL https://get.deku.sh | bash
# or
curl -fsSL https://raw.githubusercontent.com/yourorg/deku/main/scripts/install.sh | bash
```

This will:
1. Detect architecture (amd64 / arm64)
2. Download `dekud` and `deku` binaries from GitHub Releases
3. Install Angie via apt
4. Write `/etc/systemd/system/deku.service`
5. Run `deku setup --defaults --no-systemd` unless config already exists
6. Replace dashboard assets and restart `dekud` on upgrade/reinstall

---

## 16. Command Reference

See §9 (Milestone 4) for the full planned command surface.

Implemented today (CLI stubs → full implementation):
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
- `deku objectstore setup|info|test|unset`
- `deku postgres create|destroy|link|unlink|list|info|connect|logs|backup|backups|restore`
- `deku redis create|destroy|link|unlink|list|info|connect|logs`

---

## 17. Plugin Architecture

Plugins are compiled as `cdylib` crates and installed to `~/.deku/plugins/`.

Each plugin must export:
```rust
#[no_mangle]
pub extern "C" fn deku_plugin_create() -> *mut dyn PluginDescriptor {
    Box::into_raw(Box::new(MyPlugin))
}
```

The `PluginDescriptor` trait (in `dekud/src/plugins/mod.rs`) provides:
- `name()`, `version()`
- Optional hook factories: `pre_build()`, `post_build()`, `pre_deploy()`, `post_deploy()`, `app_create()`, `app_destroy()`

The `PluginRegistry` in `dekud` loads all `.so`/`.dylib` files from the plugins directory at startup and dispatches hooks at each lifecycle point.

Current repo state:
- The dynamic loader and SDK exist, but first-party service features such as Postgres and Redis are currently delivered through built-in daemon service modules and CLI commands rather than through the `cdylib` runtime
- The hook registry methods exist, but lifecycle dispatch into app/deploy flows is still incomplete, so the first-party plugin crates remain mostly lightweight placeholders today

---

## 18. deku.toml Reference

```toml
[build]
builder     = "dockerfile"        # dockerfile | nixpacks | pack | image | auto
dockerfile  = "Dockerfile"        # path override
context     = "."                 # build context path

[build.args]
NODE_ENV    = "production"

[deploy]
healthcheck = "/health"           # HTTP path for health checks
port        = 3000                # override auto-detected EXPOSE port
wait        = 5                   # seconds to wait before health checks
timeout     = 30                  # per-attempt timeout seconds
attempts    = 5                   # max health check attempts
retire      = 60                  # seconds before retiring old containers

[processes]
web         = 1
worker      = 2

[object_store]
provider          = "r2"          # r2 | s3
bucket            = "deku-prod"
region            = "auto"
endpoint          = "https://<account>.r2.cloudflarestorage.com"
access_key_id     = "..."
secret_access_key = "..."
path_style        = true
prefix            = "artifacts/"
```
