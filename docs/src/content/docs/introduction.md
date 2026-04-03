---
title: Introduction
description: What Deku is and how teams use it today.
---

Deku is a lightweight platform for deploying and operating applications on your own server with a CLI and a built-in dashboard.

## What Deku Covers

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

Start with [Installation](/installation/), then continue with [Get Started](/get-started/).

## Learn More

- Read [Dashboard Overview](/dashboard-overview/) for the user-facing web workflows.
- Read [Architecture](/architecture/) for the CLI, daemon, dashboard-auth, and API model.
- Read [CLI Reference](/reference/cli-reference/) for the command surface.
- Read [Agent Operations](/agent-operations/) if you are driving Deku from coding-agent workflows.
