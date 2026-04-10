---
title: Installation
description: How to install Deku on your server
---

Use this flow to install the packaged Deku release on a Linux server.

## Requirements

- Linux server with `apt-get` available
- Ubuntu 22.04+ or Debian 11+
- Docker Engine 24+
- 512 MB RAM minimum
- root access

## One-line Install

```bash
curl -fsSL https://get-deku.vercel.app/install.sh | sudo bash
```

`https://get-deku.vercel.app/install.sh` is the canonical public installer entrypoint for this project.

The installer will:

1. Install required system packages, then add the Angie apt repository and install Angie
2. Download the latest `dekud` and `deku` binaries plus packaged support files
3. Install the packaged systemd unit and base Angie fragment
4. Run `deku setup --no-systemd`
5. Stage the packaged dashboard bundle into the configured data directory
6. Enable and start `angie` and `deku`
7. Verify the Angie config, daemon health endpoint, and Unix socket

`dekud` expects a staged dashboard directory at runtime. The packaged installer handles that for you by unpacking the release dashboard bundle into the configured dashboard directory.

New installs default Deku's embedded SSH deploy server to port `2222`. This avoids the common case where the host's own `sshd` already occupies port `22`. CLI and API deploys do not depend on the SSH listener.

If the installer has an interactive TTY, `deku setup` prompts for values like data directory, API port, SSH port, Angie config directory, global domain, and optional object storage. Without a TTY, it runs with built-in defaults and warns that you can rerun `deku setup` later to customize settings.

## What You Get After Install

The install and setup flow gives you the dashboard access details you need:

- the server-reachable dashboard URL
- the local loopback dashboard URL for on-host access
- a one-time dashboard token shown during `deku setup`
- the `deku dashboard` command you can run later for non-secret access details
- the `deku dashboard reset-token` command to mint a new token if the original is lost

If `~/.deku/config.toml` already exists, the installer keeps the current config instead of rerunning setup.

After install, use:

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
- the config path
- token status
- SSH tunnel and firewall guidance for remote access
- the reset-token command if you need a replacement token

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

- `keep persisted data` removes the Deku service, binaries, and Deku-managed Angie config, but keeps your local config, SQLite database, dashboard assets, logs, SSH host key, Docker volumes, and the Angie package already installed on the host
- `full uninstall` removes the same host install artifacts, removes Deku-owned local state and built-in service volumes, and also purges the Angie package plus its apt metadata

Useful non-interactive variants:

```bash
deku uninstall --keep-data
deku uninstall --full-remove --yes
deku uninstall --full-remove --dry-run
```

`deku uninstall` is currently intended for the packaged Linux install path only.
