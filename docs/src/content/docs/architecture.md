---
title: Architecture
description: How the CLI, daemon, dashboard, token, and APIs fit together.
---

Runtime view of how the CLI, daemon, dashboard, and host services fit together.

## Tech Stack

Deku is built from a small Rust-first platform stack:

- [Rust](https://www.rust-lang.org/) for the `deku` CLI, the `dekud` daemon, shared core types, and plugin interfaces
- [Astro](https://astro.build/) for the dashboard and documentation frontends
- [Docker Engine](https://www.docker.com/) for app builds, app containers, and managed service containers
- [Angie](https://angie.software/) for HTTP routing, reverse proxying, and TLS termination
- [SQLite](https://www.sqlite.org/) for persisted platform state on the host
- [Bun](https://bun.sh/) for frontend package management and dashboard/docs build workflows in a source checkout

## Core Components

Deku currently consists of two binaries:

- `deku`: The CLI used for setup, dashboard access details, deploys, inspection, and day-to-day terminal workflows
- `dekud`: The server daemon that manages app state, serves the dashboard, exposes the authenticated HTTP API, and streams logs and events

## Authentication Model

- Local `deku` CLI commands prefer the trusted Unix socket and do not depend on a plaintext token file
- Dashboard and direct TCP API access use the same opaque dashboard token
- The token is shown once during setup or reset, then stored only as an Argon2id hash in config
- The dashboard stores the token in the browser after the first sign-in
- `deku dashboard reset-token` rotates the token and invalidates previous browser sessions

## API and Event Model

`dekud` exposes:

- Authenticated HTTP API routes for apps, deploys, config, domains, routing, services, storage, object store, SSH keys, plugins, and related host operations
- Server-sent event streams for deploy and runtime event output
- Log endpoints for apps and managed services

The dashboard consumes those APIs directly over TCP bearer auth. The CLI uses the same API surface locally over the trusted Unix socket.

## Runtime Components

The current server runtime includes:

- `dekud` for orchestration and API handling
- Docker Engine for app and service containers
- Angie for routing and TLS
- SQLite for persisted platform state
- The packaged dashboard bundle served by the daemon

## Source Checkout Workflow

Running Deku directly from a source checkout is a contributor or local-testing workflow, not the main packaged user flow.

If you are working from the repo, build fresh dashboard assets with `cd dashboard && bun run build`. That writes the static site to `dashboard/dist`.

`dekud` does not embed a compiled dashboard snapshot anymore. If the configured dashboard directory is missing or empty, the daemon stays online and serves a small fallback page that explains how to stage the dashboard bundle.
