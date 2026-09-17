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
- [App templates](/app-templates/) lists local starter directories you can deploy as-is.
- [Architecture](/architecture/) explains how the CLI, daemon, dashboard auth, and APIs fit together.
- [CLI Reference](/reference/cli-reference/) documents the full `deku` command surface.
- [API Reference](/reference/api-reference/) points at the live OpenAPI document and Scalar UI.
- [deku.toml](/reference/deku-toml/) documents build, deploy, and process configuration.
- [AGENTS.md](/agents/) is a copyable coding-agent workflow for operating Deku.

## Operations

- [Traffic control](/traffic-control/): maintenance mode and per-path redirects.
- [Runtime access](/runtime-access/): one-off commands and exec into a running container.
- [Resource limits](/resource-limits/): cap memory and CPU per process.
- [Backups](/backups/): on-demand and scheduled datastore backups with retention.
- [Diagnostics](/diagnostics/): `deku doctor` host checks and log streaming timeouts.

## What You Can Do

- **App lifecycle**: create apps, deploy from source archives or images, inspect deployment history, and roll back.
- **Runtime management**: manage config vars, domains, port mappings, TLS, process scale, logs, networks, storage mounts, and cron entries.
- **Traffic and resources**: take an app into maintenance, redirect individual paths, require authentication, and cap memory and CPU.
- **Built-in services**: provision Postgres, MySQL, MariaDB, Redis, and MongoDB services, link them to apps, and back them up on demand or on a schedule.
- **Operational visibility**: use `deku doctor` and the dashboard for app, routing, service, object-store, SSH-key, and plugin workflows.
