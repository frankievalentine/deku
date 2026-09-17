---
title: Deku Documentation
description: What Deku is, what it covers, and where to start.
template: splash
hero:
  title: DEKU
  tagline: A lightweight self-hosted PaaS for deploying and operating apps on your own server with a CLI and built-in dashboard.
  actions:
    - text: Install Deku
      link: /installation/
      variant: primary
    - text: Get Started
      link: /get-started/
      variant: minimal
---

## Start Here

1. [Install Deku](/installation/) on a Linux server with Docker and root access.
2. [Get Started](/get-started/) with the dashboard token, your first app, and your first deploy.

## Common Guides

- [Dashboard Overview](/dashboard-overview/) covers the web workflows for apps, deploys, routing, and services.
- [App Templates](/app-templates/) lists local starter directories you can deploy as-is.
- [Architecture](/architecture/) explains how the CLI, daemon, dashboard auth, and APIs fit together.
- [CLI Reference](/reference/cli-reference/) documents the full `deku` command surface.
- [deku.toml](/reference/deku-toml/) documents build, deploy, and process configuration.
- [AGENTS.md](/agents/) is a copyable coding-agent workflow for operating Deku.

## What You Can Do

- **App lifecycle**: create apps, deploy from source archives or images, inspect deployment history, and roll back.
- **Runtime management**: manage config vars, domains, port mappings, TLS, process scale, logs, networks, storage mounts, and cron entries.
- **Built-in services**: provision Postgres, Redis, and MySQL services and link them to apps.
- **Operational visibility**: use the dashboard for app, routing, service, object-store, SSH-key, and plugin workflows.
