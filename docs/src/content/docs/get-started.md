---
title: Get Started
description: Install Deku, open the dashboard, deploy an app, and manage it.
---

This guide walks through the first-run user flow after you have installed Deku on a server.

## 1. Get Dashboard Access Details

During `deku setup`, Deku prints a one-time dashboard token. Save it before continuing.

```bash
deku dashboard
```

This prints:

- the server-reachable dashboard URL
- the local loopback dashboard URL
- whether dashboard access is configured
- SSH tunnel and firewall guidance for remote access
- the reset command if you need a new token

If your browser is on another machine, either make TCP port `2810` reachable or use the SSH tunnel command printed by `deku dashboard`. Then open the dashboard URL in your browser and sign in with the one-time token from setup. If you lost it, run `deku dashboard reset-token`.

## 2. Create Your First App

Create an app from the CLI:

```bash
deku apps create my-app
deku apps info my-app
```

## 3. Deploy from the CLI

Deploy a source directory:

```bash
deku deploy run my-app --path /absolute/path/to/app
```

Or deploy a pre-built image:

```bash
deku deploy run my-app --image ghcr.io/acme/my-app:latest
```

After the deploy starts, inspect the result from the CLI:

```bash
deku deploy list my-app
deku apps info my-app
deku logs my-app -n 100
deku ps list my-app
```

## 4. Manage the App in the Dashboard

After the first deploy, open the app in the dashboard and continue there for day-to-day operations:

- inspect deployment history and open a deployment detail view
- manage config vars, domains, ports, routing, and TLS
- adjust scale and review live logs and events
- manage networks, storage mounts, and cron entries
- review host routing, services, object store, SSH keys, and plugins

If you want to use `git push` deploys later, Deku's embedded SSH endpoint defaults to port `2222` on new installs.

## 5. Common Next Commands

```bash
deku config set my-app NODE_ENV=production PORT=3000
deku domains add my-app app.example.com
deku ps scale my-app web=2
deku checks run my-app
deku checks routing my-app
```

Read [Dashboard Overview](/dashboard-overview/) for the web workflows and [CLI Reference](/reference/cli-reference/) for the full command surface.
