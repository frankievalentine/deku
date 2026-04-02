---
title: Introduction
description: What is Deku?
---

# Introduction

Deku is a modern, lightweight self-hosted PaaS platform inspired by Dokku.

## Key Features

- **Zero runtime dependencies** — single static Rust binary
- **CLI-first** — everything the dashboard can do, the CLI can do
- **Lightweight** — daemon idles at ~5–15 MB RSS
- **Docker-native** — container lifecycle managed via Docker Engine API
- **Plugin-first** — non-core features live in plugins

## Architecture

Deku consists of two binaries:

- `dekud` — the daemon that manages all application state, runs the SSH server, and serves the dashboard
- `deku` — the thin CLI client that talks to the daemon
