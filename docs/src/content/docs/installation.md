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
curl -fsSL https://deku.vercel.app/install.sh | bash
```

`https://deku.vercel.app/install.sh` is the canonical public installer entrypoint for this project.

The installer will:

1. Install Angie (reverse proxy)
2. Download the latest `dekud` and `deku` binaries
3. Download and stage the packaged dashboard assets
4. Install the packaged systemd unit and base Angie fragment
5. Run `deku setup`
6. Restart `dekud` with the new binaries and dashboard bundle

`dekud` expects a staged dashboard directory at runtime. The packaged installer handles that for you by unpacking the release dashboard bundle into the configured dashboard directory.

New installs default Deku's embedded SSH deploy server to port `2222`. This avoids the common case where the host's own `sshd` already occupies port `22`. CLI and API deploys do not depend on the SSH listener.

## What You Get After Install

The install and setup flow gives you the dashboard access details you need:

- the server-reachable dashboard URL
- the local loopback dashboard URL for on-host access
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

- the server-reachable dashboard URL
- the local loopback dashboard URL
- whether dashboard access is configured
- SSH tunnel and firewall guidance for remote access
- the reset command if you need a replacement token

## Dashboard Reachability

The dashboard and authenticated HTTP API are served on TCP port `2810` by default.

If your browser is running on the Deku host itself, use the local URL printed by `deku dashboard`, usually `http://127.0.0.1:2810`.

If your browser is running on another machine, either:

- use an SSH tunnel, which is the recommended default:
  `ssh -L 2810:127.0.0.1:2810 root@YOUR_SERVER_IP`
- or allow TCP `2810` through your firewall for direct browser access:
  `sudo ufw allow 2810/tcp`

If you expose `2810` directly, prefer restricting it to your own public IP instead of opening it to the world:

```bash
sudo ufw allow from YOUR_PUBLIC_IP to any port 2810 proto tcp
```

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
