---
title: Runtime access
description: Run one-off commands in a fresh container or inside a running app container.
---

`deku run` and `deku exec` reach your app's environment without a redeploy. Both stream output as it happens and exit with the command's own exit code, so they compose with shell scripts and CI.

## Run a one-off command

`deku run` creates a new container from the app's current image, runs the command, and removes the container when it finishes. Use it for migrations, data seeding, and admin tasks.

```bash
deku run my-app python manage.py migrate
deku run my-app rails db:seed
deku run my-app node scripts/report.js
```

The container receives the app's config vars and storage mounts, and nothing else: no published ports, no process scale, and no restart policy. Because it is created fresh, a `run` cannot disturb your running app. The trade-off is that anything written outside a storage mount is discarded with the container.

## Run a command in a running container

`deku exec` runs the command inside an existing web container, so it sees the live filesystem and process state.

```bash
deku exec my-app env
deku exec my-app ls -la /app
```

`exec` targets the app's `web` process. If the app has no running containers, the command fails with `app has no running containers`.

Prefer `run` for anything that changes data. `exec` writes to the live container, and the change disappears on the next deploy, so keep `exec` for inspection.

## Exit codes and output

Both commands stream stdout and stderr to your terminal and exit with the command's status. A failing command exits non-zero, so this works as expected:

```bash
deku run my-app python manage.py migrate && deku deploy run my-app --path .
```

## Interactive sessions

Neither command attaches a terminal or stdin, so they run non-interactive commands only. `deku exec my-app sh` starts a shell that immediately exits for lack of input. Run one command instead:

```bash
deku exec my-app sh -c 'cat /app/config/current.json'
```

Interactive shells are not supported yet.
