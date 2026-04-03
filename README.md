# Deku

Deku is a lightweight self-hosted PaaS for deploying and operating applications on your own server.

It combines:

- a Rust CLI: `deku`
- a Rust daemon: `dekud`
- Docker for app and service containers
- Angie for routing and TLS
- a built-in dashboard served directly by the daemon

The current workflow is intentionally practical:

- install Deku on a Linux server
- create an app
- deploy from a source directory or image
- manage config, domains, TLS, scale, logs, services, storage, networks, and cron from the CLI or dashboard

## What Deku Covers Today

- App lifecycle: create apps, deploy from source archives or images, inspect deployment history, and roll back
- Runtime management: config vars, domains, ports, routing, TLS, process scale, logs, networks, storage mounts, and cron entries
- Built-in managed services: Postgres, Redis, and MySQL
- Operational visibility: dashboard pages for apps, deployments, routing, services, object store, SSH keys, and plugins
- Backup and artifact workflows: object-store-backed Postgres and Redis backups plus deploy artifact retention
- SSH deploy ergonomics: server SSH keys, git remote helpers, and deploy feedback in both CLI and SSH flows

## Quick Start

Requirements:

- Linux server: Ubuntu 22.04+ or Debian 12+
- Docker Engine 24+
- 512 MB RAM minimum

Install on the server:

```bash
curl -fsSL https://raw.githubusercontent.com/frankievalentine/deku/main/scripts/install.sh | bash
```

After install:

```bash
deku dashboard
deku apps create my-app
deku deploy run my-app --path /absolute/path/to/app
```

Useful follow-up commands:

```bash
deku apps info my-app
deku deploy list my-app
deku logs my-app -n 100
deku ps list my-app
deku checks run my-app
deku checks routing my-app
```

## Dashboard

The dashboard is served by `dekud` itself.

Run:

```bash
deku dashboard
```

That prints:

- the dashboard URL
- whether dashboard access is configured
- reset guidance if you need a new token

The initial token is shown once during `deku setup` or `deku dashboard reset-token`.

## Core Command Surface

Current CLI groups include:

- `deku apps`
- `deku config`
- `deku deploy`
- `deku domains`
- `deku logs`
- `deku ps`
- `deku checks`
- `deku ssh`
- `deku letsencrypt`
- `deku objectstore`
- `deku postgres`
- `deku redis`
- `deku mysql`
- `deku network`
- `deku storage`
- `deku cron`
- `deku git`
- `deku plugins`

See [docs/src/content/docs/reference/cli-reference.md](docs/src/content/docs/reference/cli-reference.md) for the full command reference.

## Managed Services

Deku currently includes first-party built-in service workflows for:

- Postgres
- Redis
- MySQL

These services can be created, linked to apps, inspected, and tailed from the CLI and dashboard.

Postgres and Redis also support persisted backup workflows through the configured object store.

## Deploy Model

Deku supports:

- image deploys via `deku deploy run <app> --image <ref>`
- source deploys via `deku deploy run <app> --path <dir>`
- SSH `git push` deploys as a supported secondary path

Rollout behavior is controlled by `deku.toml`:

```toml
[deploy]
healthcheck = "/health"
port = 3000
wait = 5
timeout = 30
attempts = 5
retire = 60
```

Those settings are applied by the live deploy pipeline, not just stored as metadata.

## Current Platform Support

Current supported server/runtime path:

- Linux host
- Docker Engine
- Angie
- systemd-based packaged install

Current additive platform direction:

- Linux remains the primary production host path
- macOS CLI support is planned as a separate distribution track
- full macOS daemon/runtime support is intentionally deferred until host abstractions land

## Repository Layout

- `crates/deku`: CLI
- `crates/dekud`: daemon and HTTP API
- `crates/deku-core`: shared types and auth utilities
- `dashboard/`: Astro + React dashboard
- `docs/`: Starlight documentation site
- `plugins/`: dynamic plugin crates and experiments
- `scripts/`: installer, release packaging, and smoke tests

## Documentation

Start here:

- [Introduction](docs/src/content/docs/introduction.md)
- [Installation](docs/src/content/docs/installation.md)
- [Get Started](docs/src/content/docs/get-started.md)
- [Dashboard Overview](docs/src/content/docs/dashboard-overview.md)
- [Architecture](docs/src/content/docs/architecture.md)
- [CLI Reference](docs/src/content/docs/reference/cli-reference.md)
- [API Reference](docs/src/content/docs/reference/api-reference.md)
- [deku.toml Reference](docs/src/content/docs/reference/deku-toml.md)
- [Plugin API](docs/src/content/docs/reference/plugin-api.md)
- [Agent Operations](docs/src/content/docs/agent-operations.md)

## Development

Common verification commands:

```bash
cargo fmt --all
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Frontend and docs:

```bash
cd dashboard && bun run check
cd docs && bun run check
```

Installer smoke coverage:

```bash
./scripts/test-install-smoke.sh
```

## License

MIT
