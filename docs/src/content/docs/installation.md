---
title: Installation
description: How to install Deku on your server.
---

Use this flow to install the packaged Deku release on a Linux server.

## Requirements

- Linux server with `apt-get` available
- Ubuntu 22.04+ or Debian 11+
- Docker Engine 24+ and the `docker` CLI on `PATH`
- 512 MB RAM minimum
- Root access

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

The installer also installs `railpack`, Deku's language-aware builder for sources without a `Dockerfile`. Railpack builds with BuildKit, so `dekud` starts and manages a small `moby/buildkit` container named `deku-buildkit` on first use. Set `[buildkit] host` in the daemon config to point at an external BuildKit instead.

If the installer has an interactive TTY, `deku setup` prompts for values like data directory, API port, SSH port, Angie config directory, global domain, and optional object storage. Without a TTY, it runs with built-in defaults and warns that you can rerun `deku setup` later to customize settings.

## What You Get After Install

The install and setup flow gives you the dashboard access details you need:

- The server-reachable dashboard URL
- The local loopback dashboard URL for on-host access
- A one-time dashboard token shown during `deku setup`
- The `deku dashboard` command you can run later for non-secret access details
- The `deku dashboard reset-token` command to mint a new token if the original is lost

If a config already exists, the installer keeps it instead of rerunning setup.

## File Locations

New installs keep config and state apart:

| Path | Contents |
| --- | --- |
| `/etc/deku/config.toml` | Daemon config, including the dashboard token hash. Mode `0600`. |
| `/var/lib/deku/` | SQLite database, logs, dashboard assets, the CLI socket, and git repositories. |

Installs from before this layout keep both under `~/.deku/` (usually `/root/.deku/`), and `deku`
and `dekud` keep finding them there. Set `DEKU_CONFIG_DIR` to use any other directory.

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

- The server-reachable dashboard URL
- The local loopback dashboard URL
- The config path
- Token status
- SSH tunnel and firewall guidance for remote access
- The reset-token command if you need a replacement token

## Dashboard Reachability

The dashboard and authenticated HTTP API are served on TCP port `2810`. New installs listen on
loopback only (`api_bind = "127.0.0.1"` in `/etc/deku/config.toml`), because the dashboard is plain
HTTP and the token would otherwise cross the network unencrypted.

If your browser is running on the Deku host itself, open `http://127.0.0.1:2810`.

If your browser is running on another machine, open an SSH tunnel and then open
`http://127.0.0.1:2810` locally:

```bash
ssh -L 2810:127.0.0.1:2810 root@YOUR_SERVER_IP
```

To listen on every interface instead, install with `DEKU_API_BIND=0.0.0.0`, pass
`--api-bind 0.0.0.0` to `deku setup`, or change `api_bind` and restart `dekud`. Then restrict the
port to your own address rather than opening it to the world:

```bash
sudo ufw allow from YOUR_PUBLIC_IP to any port 2810 proto tcp
```

Installs from before `api_bind` existed have no such line and keep listening on every interface.
`deku doctor` warns about it.

### Publish the dashboard through Angie

To reach the dashboard at a hostname without a tunnel, set `dashboard_domain` and restart `dekud`:

```toml
global_domain = "apps.example.com"
dashboard_domain = "deku.apps.example.com"
```

Angie then serves the dashboard over HTTPS at that hostname and redirects HTTP to it. It is only
ever published over HTTPS, with the global domain's [wildcard certificate](/certificates/), so:

- `dashboard_domain` must be one label below `global_domain`
- the wildcard certificate must be enabled; the dashboard is published once Angie has issued it
- the hostname must not be one of your apps' domains

`deku doctor` reports whether the dashboard is published and, if not, why. Sign-in attempts through
the published hostname are rate-limited per client address.

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
