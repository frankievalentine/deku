---
title: Installation
description: How to install Deku on your server
---

This page covers the packaged server install path for Deku.

## Requirements

- Linux server (Ubuntu 22.04+ or Debian 12+)
- Docker Engine 24+
- 512 MB RAM minimum

## One-line Install

```bash
curl -sSL https://get.deku.sh | bash
# or
curl -fsSL https://raw.githubusercontent.com/frankievalentine/deku/main/scripts/install.sh | bash
```

The installer will:

1. Install Angie (reverse proxy)
2. Download the latest `dekud` and `deku` binaries
3. Download and stage the packaged dashboard assets
4. Install the packaged systemd unit and base Angie fragment
5. Run `deku setup`
6. Restart `dekud` with the new binaries and dashboard bundle

## What You Get After Install

The install and setup flow gives you the dashboard access details you need:

- the dashboard URL
- a one-time dashboard token shown during `deku setup`
- the `deku dashboard` command you can run later for non-secret access details
- the `deku dashboard reset-token` command to mint a new token if the original is lost

Example:

```bash
deku dashboard
```

## After Install

Useful first checks from the server:

```bash
deku dashboard
deku apps list
deku apps info <app>
```

`deku dashboard` prints:

- the dashboard URL
- whether dashboard access is configured
- the reset command if you need a replacement token

If you lost the original one-time token:

```bash
deku dashboard reset-token
```

Continue with [Get Started](/get-started/) for the first app flow, or read [Dashboard Overview](/dashboard-overview/) for the web interface.

## Uninstall

Use the packaged uninstall command from the server:

```bash
deku uninstall
```

The command is root-only and interactive by default. It supports two modes:

- `keep persisted data` removes the Deku service, binaries, and Deku-managed Angie config, but keeps your local config, SQLite database, dashboard assets, logs, SSH host key, and Docker volumes
- `full uninstall` removes the same host install artifacts and also removes Deku-owned local state and built-in service volumes

Useful non-interactive variants:

```bash
deku uninstall --keep-data
deku uninstall --full-remove --yes
deku uninstall --full-remove --dry-run
```

`deku uninstall` is currently intended for the packaged Linux install path only. It does not uninstall Angie itself.
