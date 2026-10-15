---
title: Dashboard Overview
description: What the Deku dashboard covers and how to use it.
---

The dashboard is the main web interface for operating apps and host-level features in Deku.

It is served directly by `dekud` from the configured dashboard directory. In the packaged install path, the installer stages the built dashboard assets for you from the release bundle.

The screenshot shows the redesigned dashboard, which replaces the current one in an upcoming
release. The pages below describe the dashboard that ships today.

<figure>
  <img class="dark:sl-hidden" src="/images/dashboard-apps-light.webp" alt="The Deku dashboard apps list, showing each app's status, current release, process health, and recent deploys." width="2400" height="1017" loading="lazy" decoding="async" />
  <img class="light:sl-hidden" src="/images/dashboard-apps-dark.webp" alt="The Deku dashboard apps list, showing each app's status, current release, process health, and recent deploys." width="2400" height="1017" loading="lazy" decoding="async" />
</figure>

## Accessing the Dashboard

Run:

```bash
deku dashboard
```

That command prints:

- The server-reachable dashboard URL derived from local CLI config and host IP detection
- The local loopback dashboard URL for on-host access
- Whether dashboard access is configured
- SSH tunnel and firewall guidance for remote access
- The reset command if a replacement token is needed

If your browser is on another machine, use the printed SSH tunnel command: the dashboard listens on loopback only by default (see [Installation](/installation/#dashboard-reachability)). Then open the dashboard URL in a browser and sign in with the one-time dashboard token shown during `deku setup` or a later `deku dashboard reset-token`.

## Pages

The sidebar has these pages:

- **Apps**: every app with its status. Create an app here, or open one.
- **Services**: create Postgres, MySQL, MariaDB, Redis and MongoDB services, see their connection
  values and container output, link them to apps, back them up, restore a backup, and set an
  automatic backup schedule.
- **Storage**: connect and test the S3-compatible object store used for backups.
- **Routing**: the routing table for every app, the proxy configuration, automatic certificates,
  and the Let's Encrypt account email.
- **Overview**: host checks, active alerts, Docker networks, and the host event feed.
- **SSH keys**: add and remove the public keys allowed to `git push` deploys.
- **Settings**: the dashboard token (copy, rotate, or disconnect this browser) and the installed
  and latest Deku versions.
- **API**: the interactive API reference, generated from the daemon's OpenAPI document.

## App pages

Opening an app shows these tabs:

- **Deploy**: deploy an image or upload an archive, roll back, browse deployments, run health
  checks, manage environments, and create deploy tokens for CI.
- **Domains**: domains and certificates, ports and upstreams, and the app's routing status and
  proxy files.
- **Traffic**: maintenance mode, redirects, and basic or forward auth.
- **Processes**: scale each process type, set memory and CPU limits, and run a command in a fresh
  or running container.
- **Config**: config vars, app-wide or per environment, and object store credentials for the app.
- **Infrastructure**: attached Docker networks, storage mounts, and scheduled commands.
- **Logs**: live build and runtime output, with search.
- **Settings**: app details and deleting the app.

Selecting a deployment opens its detail page: each phase of the rollout, its source, the event
stream, and rolling back to it.

## Only in the CLI

These are configured with the CLI or the config file rather than the dashboard: lifecycle hooks,
the build host, the image registry, git remote settings, and `deku doctor` itself (the Overview page
shows the same checks).

For the underlying HTTP surface, read [API Reference](/reference/api-reference/). For the runtime model behind the dashboard, read [Architecture](/architecture/).
