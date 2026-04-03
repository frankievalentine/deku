---
title: Self-hosting
description: Notes for running Deku on your own server.
---

Deku is intended for a single-server self-hosted deployment model.

## Minimum Requirements

- Ubuntu 22.04+ or Debian 11+
- 1 vCPU
- 512 MB RAM
- 10 GB disk
- Docker Engine 24+
- Angie latest stable

## Current Runtime Shape

Core moving parts:

- `dekud` for API, events, deploy orchestration, SSH, and dashboard serving
- SQLite for local state
- Docker Engine for containers
- Angie for reverse proxy and TLS

## Local Ops Note

At the current milestone state:

- the daemon and dashboard share the same listener port in local testing
- the dashboard requires a manual build and copy step into `~/.deku/dashboard`
- installation and packaging are not yet complete

## Recommended Current Test Loop

1. Build dashboard assets.
2. Copy them into `~/.deku/dashboard`.
3. Run `RUST_LOG=info cargo run -p dekud`.
4. Use the dashboard plus CLI together to verify state transitions.
