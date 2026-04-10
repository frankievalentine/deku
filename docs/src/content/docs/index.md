---
title: Deku Documentation
description: What Deku is, what it covers, and where to start.
template: splash
hero:
  title: DEKU
  tagline: A lightweight self-hosted PaaS for deploying and operating apps on your own server with a CLI and built-in dashboard.
  actions:
    - text: Installation
      link: /installation/
    - text: Get Started
      link: /get-started/
    - text: Dashboard Overview
      link: /dashboard-overview/
---

## What You Can Do With Deku

- **App lifecycle**: create apps, deploy from source archives or images, inspect deployment history, and roll back when needed
- **Runtime management**: manage config vars, domains, port mappings, TLS, process scale, logs, networks, storage mounts, and cron entries
- **Built-in services**: provision Postgres, Redis, and MySQL services and link them to apps
- **Operational visibility**: use the dashboard for app, routing, service, object-store, SSH-key, and plugin workflows
- **CLI-first workflows**: use `deku` from the terminal for setup, deploys, inspection, and automation-friendly operations

## Typical Workflow

1. Install Deku on a server.
2. Save the one-time dashboard token shown during setup, then run `deku dashboard` later if you need the URL or reset guidance.
3. Create an app and deploy it from the CLI.
4. Open the dashboard to inspect deployments, manage runtime settings, and monitor the host.

## Key Guides

- Start with [Installation](/installation/).
- Follow [Get Started](/get-started/) for the first app and first deploy workflow.
- Read [Dashboard Overview](/dashboard-overview/) before managing apps from the web UI.
- Read [Architecture](/architecture/) for the CLI, daemon, dashboard-auth, and API model.
- Use [CLI Reference](/reference/cli-reference/) and [deku.toml](/reference/deku-toml/) when working from the terminal.
- Use [AGENTS.md](/agents/) for a copyable coding-agent workflow for Deku operations.
