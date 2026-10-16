---
title: Architecture
description: How the CLI, daemon, dashboard, token, and APIs fit together.
---

Runtime view of how the CLI, daemon, dashboard, and host services fit together.

## Tech Stack

Deku is built from a small Rust-first platform stack:

- [Rust](https://www.rust-lang.org/) for the `deku` CLI, the `dekud` daemon, and shared core types
- [React](https://react.dev/) and [Vite](https://vite.dev/) for the dashboard, a single-page app `dekud` serves
- [Astro](https://astro.build/) with Starlight for the documentation site
- [Docker Engine](https://www.docker.com/) for app builds, app containers, and managed service containers
- [BuildKit](https://github.com/moby/buildkit) as the build engine for Railpack language builds
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
- The dashboard sends the token once at sign-in and gets an `HttpOnly` session cookie back; the
  token itself is not kept in the browser
- `deku dashboard reset-token` rotates the token and ends every other browser session
- Dashboard tokens expire 30 days after they are issued; rotate with `deku dashboard reset-token`
- The dashboard is served over plain HTTP, so new installs listen on loopback only and remote
  access goes through an SSH tunnel

## API and Event Model

`dekud` exposes:

- Authenticated HTTP API routes for apps, deploys, config, domains, routing, services, storage, object store, SSH keys, and related host operations
- Server-sent event streams for deploy and runtime event output
- Log endpoints for apps and managed services

The dashboard calls those APIs over TCP with its session cookie, and keeps one event stream open per tab. Scripts use the same routes with the token as a bearer header. The CLI uses the same API surface locally over the trusted Unix socket.

## Runtime Components

The current server runtime includes:

- `dekud` for orchestration and API handling
- Docker Engine for app and service containers
- A managed BuildKit container (`deku-buildkit`) for Railpack language builds
- Angie for routing and TLS
- SQLite for persisted platform state
- The packaged dashboard bundle served by the daemon

## Container Networking

Every app and managed service container joins one Docker bridge network named `deku`.

- Linked apps reach a managed service by its container name, for example
  `postgres://deku:…@deku-postgres-app-db:5432/app-db`. Services publish no host port, so a
  database is never reachable from outside the host.
- Each web replica publishes one port bound to `127.0.0.1`, which Angie proxies to. Traffic can
  only reach an app through Angie, so app authentication, maintenance mode, and redirects always
  apply. Docker writes its own firewall rules, which is why a port bound to every interface would
  be reachable even behind UFW.
- Release tasks and `deku run` containers join the same network, so migrations reach linked
  services.

Services created by an earlier release are moved onto the network when `dekud` starts. Each one
is recreated on its existing data volume, and linked apps get the new URL on their next deploy.
`deku doctor` reports any container that still publishes a port beyond loopback.

## Source Checkout Workflow

Running Deku directly from a source checkout is a contributor or local-testing workflow, not the main packaged user flow.

If you are working from the repo, build fresh dashboard assets with `cd dashboard && bun run build`. That writes the single-page app to `dashboard/dist`; `dekud` serves its files and answers any other non-API path with `index.html`, so a reload on a dashboard URL loads the app.

`dekud` does not embed a compiled dashboard snapshot anymore. If the configured dashboard directory is missing or empty, the daemon stays online and serves a small fallback page that explains how to stage the dashboard bundle.
