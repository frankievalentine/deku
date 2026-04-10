---
title: Dashboard Overview
description: What the Deku dashboard covers and how to use it.
---

The dashboard is the main web interface for operating apps and host-level features in Deku.

It is served directly by `dekud` from the configured dashboard directory. In the packaged install path, the installer stages the built dashboard assets for you from the release bundle.

## Accessing the Dashboard

Run:

```bash
deku dashboard
```

That command prints:

- the server-reachable dashboard URL derived from local CLI config and host IP detection
- the local loopback dashboard URL for on-host access
- whether dashboard access is configured
- SSH tunnel and firewall guidance for remote access
- the reset command if a replacement token is needed

If your browser is on another machine, either use the printed SSH tunnel command or make TCP port `2810` reachable through your firewall. Then open the dashboard URL in a browser and sign in with the one-time dashboard token shown during `deku setup` or a later `deku dashboard reset-token`.

## Pages and Workflows

- **Apps**: review the app fleet, create new apps, remove old apps, and jump directly into app workspaces
- **App detail**: manage config vars, domains, ports, routing, TLS, scale, deployment history, and live logs for a single app
- **Deployment detail**: inspect rollout history, lifecycle events, builder details, and rollback targets
- **Host overview**: review platform-wide app status, routing state, TLS configuration, object store status, services, SSH keys, plugins, and recent events
- **Routing**: inspect the routing table, validate proxy state, and manage the global Let’s Encrypt email setting
- **Services**: create and inspect Postgres, Redis, and MySQL services, link them to apps, and manage Postgres backups and restores
- **Object Store**: configure and test the S3-compatible object store used for host-level backups, deploy artifact retention, and app credential linking
- **SSH Keys**: add and remove trusted public keys for server access workflows
- **Plugins**: inspect loaded plugins and load or unload plugin libraries by path

## What You Can Manage Here

- app creation, inspection, and deletion
- deploy history, image/archive deploy follow-up, and rollback review
- config vars, domains, ports, routing, and TLS
- process scale, live logs, and deployment events
- networks, storage mounts, and cron entries
- managed services and Postgres backup workflows
- object store, SSH keys, and plugin inventory

For the underlying HTTP surface, read [API Reference](/reference/api-reference/). For the runtime model behind the dashboard, read [Architecture](/architecture/).
