# Deku

<p><sub><code>deku</code>: 9MB, <code>dekud</code>: 16MB, <code>dashboard bundle</code>: 848 KB</sub></p>

Deku is a lightweight self-hosted PaaS for deploying and operating applications on your own server.

It combines:

- A Rust CLI: `deku`
- A Rust daemon: `dekud`
- Docker for app and service containers
- Angie for routing and TLS
- A built-in dashboard served directly by the daemon

The current workflow is intentionally practical:

- Install Deku on a Linux server
- Create an app
- Start from a local starter template when you want a ready-made app skeleton
- Deploy from a source directory or image
- Manage config, domains, TLS, scale, logs, services, storage, networks, and cron from the CLI or dashboard

## What You Can Do With Deku

- App lifecycle: create apps, deploy from source archives or images, inspect deployment history, and roll back when needed
- Starter templates: begin from local framework templates adapted for Deku deploys
- Runtime management: manage config vars, domains, port mappings, TLS, process scale, logs, networks, storage mounts, and cron entries
- Built-in services: provision Postgres, Redis, and MySQL services and link them to apps
- Operational visibility: use the dashboard for app, routing, service, object-store, SSH-key, and plugin workflows
- CLI-first workflows: use `deku` from the terminal for setup, deploys, inspection, and automation-friendly operations

## Quick Start

Requirements:

- Linux server with `apt-get` available
- Ubuntu 22.04+ or Debian 11+
- Docker Engine 24+
- 512 MB RAM minimum
- Root access

Install on the server:

```bash
curl -fsSL https://get-deku.vercel.app/install.sh | sudo bash
```

Uninstall guidance, including `deku uninstall`, lives in [docs/src/content/docs/installation.md](docs/src/content/docs/installation.md).

After install:

```bash
deku dashboard
deku apps create my-app
deku deploy run my-app --path /absolute/path/to/app
```

New installs bind Deku's optional SSH deploy server to port `2222` by default so the API/dashboard can coexist with a standard host `sshd` on port `22`.

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

- The server-reachable dashboard URL
- The local loopback dashboard URL for on-host access
- The config path
- The token status
- SSH tunnel and firewall guidance for remote access
- The reset-token command if you need a replacement token

The initial token is shown once during `deku setup` or `deku dashboard reset-token`.

The dashboard and authenticated HTTP API listen on TCP port `2810` by default. If your browser is on another machine, either use an SSH tunnel or allow `2810/tcp` through your firewall.

## Managed Services

Deku currently includes first-party built-in service workflows for:

- Postgres
- Redis
- MySQL

These services can be created, linked to apps, inspected, and tailed from the CLI and dashboard.

Postgres and Redis also support persisted backup workflows through the configured object store.

## Deploy Model

Deku supports:

- Image deploys via `deku deploy run <app> --image <ref>`
- Source deploys via `deku deploy run <app> --path <dir>`
- SSH `git push` deploys as a supported secondary path

Starter templates live under [templates/](templates/) in this repository. They are adapted for Deku's `dockerfile` and `nixpacks` deploy paths.

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

## Repository Layout

- `crates/deku`: CLI
- `crates/dekud`: daemon and HTTP API
- `crates/deku-core`: shared types and auth utilities
- `dashboard/`: Astro + React dashboard
- `docs/`: Starlight documentation site
- `plugins/`: first-party plugin crates plus dynamic plugin runtime support
- `scripts/`: installer, release packaging, and smoke tests
- `templates/`: local starter app catalog for common framework deploys

## Development

Common verification commands:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --all
```

Full local CI gate:

```bash
./scripts/ci-local.sh
```

Frontend and docs:

```bash
cd dashboard && bun install --frozen-lockfile
cd dashboard && bun run lint
cd dashboard && bun run check
cd dashboard && bun run build
cd docs && bun install --frozen-lockfile
cd docs && bun run check
cd docs && bun run build
```

The dashboard build output lives in `dashboard/dist`. Packaged installs consume that output through the release `deku-dashboard.tar.gz` artifact rather than embedding a compiled dashboard snapshot into `dekud`.

## License

MIT
