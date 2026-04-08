---
title: Architecture
description: How the CLI, daemon, dashboard, token, and APIs fit together.
---

## Core Components

Deku currently consists of two binaries:

- `deku`: the CLI used for setup, dashboard access details, deploys, inspection, and day-to-day terminal workflows
- `dekud`: the server daemon that manages app state, serves the dashboard, exposes the authenticated HTTP API, and streams logs and events

## Dashboard Delivery

The dashboard is served by `dekud` from the configured dashboard directory. In the packaged install path, the installer stages the built dashboard assets for you from the release bundle.

When you run:

```bash
deku dashboard
```

the CLI prints:

- the server-reachable dashboard URL derived from local CLI config and host IP detection
- the local loopback dashboard URL for on-host access
- whether dashboard access is configured
- SSH tunnel and firewall guidance for remote access
- the reset command if a replacement token is needed

## Authentication Model

- local `deku` CLI commands prefer the trusted Unix socket and do not depend on a plaintext token file
- dashboard and direct TCP API access use the same opaque dashboard token
- the token is shown once during setup or reset, then stored only as an Argon2id hash in config
- the dashboard stores the token in the browser after the first sign-in
- `deku dashboard reset-token` rotates the token and invalidates previous browser sessions

## API and Event Model

`dekud` exposes:

- authenticated HTTP API routes for apps, deploys, config, domains, routing, services, storage, object store, SSH keys, plugins, and related host operations
- server-sent event streams for deploy and runtime event output
- log endpoints for apps and managed services

The dashboard consumes those APIs directly over TCP bearer auth. The CLI uses the same API surface locally over the trusted Unix socket.

## Runtime Components

The current server runtime includes:

- `dekud` for orchestration and API handling
- Docker Engine for app and service containers
- Angie for routing and TLS
- SQLite for persisted platform state
- the packaged dashboard bundle served by the daemon

## Source Checkout Workflow

Running Deku directly from a source checkout is a contributor or local-testing workflow, not the main packaged user flow.

If you are working from the repo, build fresh dashboard assets with `cd dashboard && bun run build`. That writes the static site to `dashboard/dist`.

`dekud` does not embed a compiled dashboard snapshot anymore. If the configured dashboard directory is missing or empty, the daemon stays online and serves a small fallback page that explains how to stage the dashboard bundle.
