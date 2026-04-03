---
title: Introduction
description: What is Deku?
---

Deku is a modern, lightweight self-hosted PaaS platform inspired by Dokku.

## Key Features

- **Integrated control plane** — single Rust daemon with Docker, Angie, and Git system integrations
- **CLI-first** — everything the dashboard can do, the CLI can do
- **Lightweight** — daemon idles at ~5–15 MB RSS
- **Docker-native** — container lifecycle managed via Docker Engine API
- **SSH deploy + direct deploy** — `git push`, source archive, and image deploy paths
- **Extension-ready** — hooks and plugin surfaces exist, but the platform is integrated first

## Architecture

Deku consists of two binaries:

- `dekud` — the daemon that manages all application state, runs the SSH server, and serves the dashboard
- `deku` — the thin CLI client that talks to the daemon

## Agent Support Today

The current agent integration model is aimed at coding agents already running in a shell or repo:

- prefer the `deku` CLI first
- use the daemon HTTP API as the machine-readable fallback
- prefer archive or image deploys over `git push` for agent workflows

See [Agent Operations](/agent-operations/) for the supported workflow.
