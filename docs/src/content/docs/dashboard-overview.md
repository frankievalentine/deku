---
title: Dashboard Overview
description: What the Deku dashboard covers, how signing in works, and what stays in the CLI.
---

The dashboard is the web interface for deploying and running apps on a Deku host. `dekud` serves it
from the configured dashboard directory; the installer stages it there from the release bundle.

<figure>
  <img class="dark:sl-hidden" src="/images/dashboard-apps-light.webp" alt="The Deku dashboard apps list, showing each app's status, current release, process health, and recent deploys." width="2400" height="1017" loading="lazy" decoding="async" />
  <img class="light:sl-hidden" src="/images/dashboard-apps-dark.webp" alt="The Deku dashboard apps list, showing each app's status, current release, process health, and recent deploys." width="2400" height="1017" loading="lazy" decoding="async" />
</figure>

## Opening it

```bash
deku dashboard
```

That prints the dashboard URL, whether a token is configured, an SSH tunnel command for reaching it
from another machine, and the command to reset the token. The dashboard listens on loopback by
default (see [Installation](/installation/#dashboard-reachability)), so from your laptop, run the
tunnel command and open the printed URL.

## Signing in

Paste the dashboard token shown by `deku setup` or `deku dashboard reset-token`. The browser sends
it once and gets a session cookie back:

- The cookie is `HttpOnly` and `SameSite=Strict`, so page scripts cannot read it and other sites
  cannot send it. It is marked `Secure` when the dashboard is served over HTTPS through Angie.
- The token itself is not stored in the browser.
- A session ends when the token expires, or after 30 days for a token without an expiry.
  **Sign out of this browser** in Settings ends it at once.
- Rotating the token ends every session except the one that rotated it, and invalidates the old
  token for scripts too.

Changes made from the dashboard also need a header only the dashboard's own pages send, and an
`Origin` that matches the host, so another page cannot make changes with the cookie.

## Live updates

Each browser tab keeps one connection to the daemon. Deploy steps, logs, certificate changes and
work done from the CLI or a `git push` appear without a reload. The connection closes while the tab
is in the background, and the page refreshes what it shows when you come back. The sidebar shows
**Live updates on** while it is connected.

## Pages

- **Apps**: every app with its status, current release, processes, and latest deploy. Create an app
  here.
- **Services**: create Postgres, MySQL, MariaDB, Redis and MongoDB services. Open one to see its
  connection values and recent output, link it to an app, back it up now, restore a backup, and set
  an automatic backup schedule.
- **Host**: the `deku doctor` checks, the routing table for every hostname, the proxy
  configuration's validity, Docker networks, recent activity, and counts of apps, containers and
  services.
- **Settings**: theme, the dashboard token (rotate it or sign out), installed and latest versions,
  certificates (automatic certificates for app domains, the certificate email, and the wildcard
  certificate for generated hostnames), the object store, and SSH keys for `git push`.

Press <kbd>⌘</kbd> <kbd>K</kbd> (<kbd>Ctrl</kbd> <kbd>K</kbd> elsewhere) to jump to an app or page,
deploy, roll back, switch theme, or open the API reference.

## App pages

Opening an app shows five tabs:

- **Overview**: the live release, processes, recent output, domains, linked services, recent deploys,
  a health check, and a box to run a command in a fresh or the running container.
- **Deploys**: deploy an image or upload a source archive, browse every deploy, and roll back. A
  deploy's page shows each step (build, release, health check, live) with its timing, and the output
  of the build and the release command as it streams.
- **Logs**: live runtime output. Filter the lines, or narrow to one stream or process.
- **Config**: config vars, app-wide or overridden per environment, plus the values Deku sets for
  linked services and the object store.
- **Settings**: domains and their certificate state, ports, basic or forward auth, redirects,
  maintenance mode, process scale and limits, environments, deploy tokens for CI, storage mounts and
  the object store link, Docker networks, scheduled jobs, and deleting the app.

## Only in the CLI

These are set with the CLI or the config file: lifecycle hooks, the build host, the image registry,
git remote settings, the global domain, and active alerts (`deku alerts`).

For the underlying HTTP surface, read [API Reference](/reference/api-reference/), or open
`/api/docs` on the host for the interactive reference. For the runtime model behind the dashboard,
read [Architecture](/architecture/).
