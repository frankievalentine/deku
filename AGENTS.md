# AGENTS

Agent operating guide for Deku.

This file is for coding agents with shell access on the same machine as the Deku project or server. It covers what to use, what to avoid, and the safest default path for deploying and inspecting apps.

Do not read this file as permission for broad unattended platform administration.

## Supported Agent Model

- Primary interface: the `deku` CLI
- Fallback machine interface: the daemon HTTP API, with an operator-provisioned dashboard token
- Preferred deploy path: archive or image deploy through `deku deploy run`
- Secondary deploy path: `git push` over SSH
- Default workflow: deploy, then inspect

## Preferred Workflow

1. Confirm `dekud` is running and reachable.
2. Inspect or create the target app.
3. Set the config vars the app needs.
4. Deploy with `deku deploy run <app> --path <dir>` or `--image <ref>`.
5. Read the deploy output to the end. A failed deploy names the failing step, and the previous version keeps serving.
6. Verify before moving on: `deku deploy list <app>`, `deku logs <app> -n 100`, `deku doctor`.
7. Stop when the app reports `deployed` and the newest deployment reports `live`.

Prefer deterministic, non-interactive commands. Use the CLI over the trusted Unix socket; use direct HTTP only when you already have a dashboard token. Use `git push` only when you specifically need to exercise the SSH remote path.

## Preflight Checks

Run these before a deploy. They read state and change nothing:

```bash
deku doctor
deku apps info <app>
deku config list <app>
```

`deku doctor` exits `1` when a check has failed. A non-zero exit means the host is not healthy: fix it before deploying.

If the app does not exist yet, create it:

```bash
deku apps create <app>
```

## Starter App Catalog

Check the local `templates/` directory in this repository before building an app from scratch:

```bash
deku deploy run <app> --path /absolute/path/to/repo/templates/node-express
deku deploy run <app> --path /absolute/path/to/repo/templates/nextjs
deku deploy run <app> --path /absolute/path/to/repo/templates/django
```

The templates are adapted for Deku: they use the `dockerfile` or `railpack` builder, expect Deku-managed services instead of bundled sidecars, and stay local to the repo so you can inspect and copy them.

## Common Operations

Create and configure an app:

```bash
deku apps create <app>
deku apps info <app>
deku apps destroy <app>          # the CLI verb is destroy, not delete
deku config set <app> KEY=VALUE
deku config list <app>
deku domains add <app> example.test
```

Deploy and roll back:

```bash
deku deploy run <app> --path /absolute/path/to/source
deku deploy run <app> --image nginx:alpine
deku deploy list <app>
deku deploy rollback <app>
```

Put migrations in a `release:` entry in the image's Procfile. That entry runs on every deploy, and a failure fails the deploy, so a broken migration never reaches the running app. Use `deku run` only for one-off tasks you trigger by hand.

Inspect a running app:

```bash
deku logs <app> -n 100
deku logs <app> --follow --timeout 30   # stop after 30 seconds without output
deku ps list <app>
deku apps info <app>
```

Run a command in the app's environment. Both forms are non-interactive, stream output, and exit with the command's status:

```bash
deku run <app> python manage.py migrate   # fresh container, removed when it exits
deku exec <app> ls -la /app               # inside the running web container
```

Use `exec` for inspection only. It writes to the live container, and the change is lost on the next deploy.

Control traffic and resources:

```bash
deku maintenance on <app> --message "Deploying"
deku maintenance off <app>
deku redirects add <app> /old /new --code 301
deku redirects list <app>
deku ps limits <app> --memory 512m --cpu 0.5
```

Turn on maintenance mode before a change that briefly breaks the app, and turn it off once the deploy reports `live`.

Back up a managed service:

```bash
deku postgres backup <service>
deku postgres backups <service>
deku postgres restore <service> <backup-id>
deku backup schedule <service> --interval-hours 24 --keep 7
deku backup schedules
```

Backups need a configured object store. Check it with `deku objectstore test` before you rely on one.

An operator may have offloaded builds to another host. Check before assuming a deploy builds locally:

```bash
deku build-host info
deku build-host check
```

`deku deploy run <app> --path <dir> --build-host local` forces a local build for one deploy.

## HTTP API Fallback

Use the daemon API when you need machine-structured output and already have a dashboard token. `deku dashboard` prints the base URL and the current token status.

```bash
TOKEN="dku_REPLACE_WITH_OPERATOR_TOKEN"
BASE="http://127.0.0.1:2810"

curl -H "Authorization: Bearer ${TOKEN}" "${BASE}/api/apps"
curl -H "Authorization: Bearer ${TOKEN}" "${BASE}/api/doctor"
curl -H "Authorization: Bearer ${TOKEN}" "${BASE}/api/apps/<app>/deployments"

curl -X POST -H "Authorization: Bearer ${TOKEN}" -H "Content-Type: application/json" \
  -d '{"source":"image","image":"nginx:alpine"}' \
  "${BASE}/api/apps/<app>/deploy"

tar -czf /tmp/app.tar.gz -C /absolute/path/to/app .
curl -X POST -H "Authorization: Bearer ${TOKEN}" \
  -F archive=@/tmp/app.tar.gz \
  "${BASE}/api/apps/<app>/deploy/archive"
```

Every route is documented at `/api/docs`, with the OpenAPI 3.1 document at `/api/openapi.json`.

## Failure Handling

If a `deku` command cannot reach the daemon, `dekud` is stopped or listening somewhere else. Start or restart it, then run `deku doctor` to confirm the database, Docker, and proxy config are healthy.

If an app does not exist, create it with `deku apps create <app>`.

If a deploy fails, the deploy output names the failing step. Then:

- `deku deploy list <app>` shows whether the previous deployment is still `live`
- `deku logs <app> -n 100` shows the container's own output
- `deku doctor` rules out the host underneath it
- Confirm config vars, domains, and process scale are correct

If a feature is missing, treat it as a precondition rather than assuming it is configured, and verify it:

```bash
deku objectstore info          # object store, required for backups
deku build-host info           # build host, for offloaded builds
deku letsencrypt status <app>  # TLS certificates
```

## What Not To Do

- Do not make `git push` the default deployment path
- Do not use the dynamic plugin runtime for first-party workflows: it is compiled out by default, and HTTP lifecycle hooks are the supported integration point
- Do not attempt broad unattended platform administration
- Do not assume MCP exists today
- Do not run interactive commands: `deku run` and `deku exec` do not attach a terminal

## Scope Today

Supported today:

- App create, inspect, and destroy
- Config vars, domains, and ports
- Deploy run, list, and rollback
- Logs and process inspection
- Runtime access with `deku run` and `deku exec`
- Maintenance mode and redirects
- Resource limits
- Object-store and service backups, on demand and on a schedule
- Host diagnostics with `deku doctor`
- Managed Postgres, MySQL, MariaDB, Redis, and MongoDB services

Out of scope today:

- Plugin authoring with a dynamic `cdylib`
- MCP implementation
- Broad unattended platform administration

## MCP Later

The CLI and daemon HTTP API already cover the coding-agent workflow, so MCP is not required today.

Revisit MCP when external assistants without shell access need structured access to a stable subset: apps, deploy, config, domains, logs, and deployments. Leave dynamic plugin surfaces out of any first design.
