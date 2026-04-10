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

<p class="docs-footprint-note"><code>deku</code>: 9MB, <code>dekud</code>: 16MB, <code>dashboard bundle</code>: 848 KB</p>

## What You Can Do With Deku

- **App lifecycle**: Create apps, deploy from source archives or images, inspect deployment history, and roll back when needed
- **Starter templates**: Begin from local framework templates adapted for Deku deploys
- **Runtime management**: Manage config vars, domains, port mappings, TLS, process scale, logs, networks, storage mounts, and cron entries
- **Built-in services**: Provision Postgres, Redis, and MySQL services and link them to apps
- **Operational visibility**: Use the dashboard for app, routing, service, object-store, SSH-key, and plugin workflows
- **CLI-first workflows**: Use `deku` from the terminal for setup, deploys, inspection, and automation-friendly operations

## Typical Workflow

1. Install Deku on a server.
2. Save the one-time dashboard token shown during setup, then run `deku dashboard` later if you need the URL or reset guidance.
3. Create an app and deploy it from the CLI.
4. Open the dashboard to inspect deployments, manage runtime settings, and monitor the host.

## Key Guides

- Start with [Installation](/installation/).
- Follow [Get Started](/get-started/) for the first app and first deploy workflow.
- Use [App Templates](/app-templates/) when you want a local starter app directory to deploy right away.
- Read [Dashboard Overview](/dashboard-overview/) before managing apps from the web UI.
- Read [Architecture](/architecture/) for the CLI, daemon, dashboard-auth, and API model.
- Use [CLI Reference](/reference/cli-reference/) and [deku.toml](/reference/deku-toml/) when working from the terminal.
- Use [AGENTS.md](/agents/) for a copyable coding-agent workflow for Deku operations.
