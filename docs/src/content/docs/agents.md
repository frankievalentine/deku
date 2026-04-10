---
title: AGENTS.md
description: How coding agents should deploy and inspect apps with Deku, plus a copyable AGENTS.md file.
---

Recommended operating contract for coding agents using Deku from a repo or shell.

The full checked-in `AGENTS.md` from the repository root is included at the bottom of the page so users can copy it directly.

If an agent needs a known-good starter app quickly, use the local [App Templates](/app-templates/) catalog before building a fresh project from scratch.

## Supported Agent Contract

Defaults:

- Primary interface: `deku` CLI
- Fallback machine interface: Daemon HTTP API with an operator-provisioned dashboard token
- Preferred deploy method: Archive or image deploy via CLI or HTTP API
- Secondary deploy method: `git push` over SSH
- Supported operator model: Coding agents running in a shell or repo
- Supported workflow class: Deploy + inspect

Supported today:

- App create / inspect
- Config set / list / unset
- Deploy run / list / rollback
- Domains add / list / remove
- Logs and process/status inspection

Out of scope today:

- Full autonomous platform administration
- Dynamic plugin authoring
- MCP implementation

## Why CLI + API First

The CLI and daemon API already cover the core workflow agents need today.

For most agent-driven deployments, archive and image deploys are a better fit than `git push` because they are:

- Easier to script deterministically
- Easier to reason about from an agent loop
- Easier to inspect and retry

SSH `git push` deploys remain supported, but they are not the recommended default path for agents.

On packaged installs, Deku's embedded SSH deploy endpoint defaults to port `2222` so the daemon can coexist with a host `sshd` on port `22`.

## Canonical Agent Workflow

The standard agent flow is:

1. Ensure `dekud` is reachable.
2. Prefer the local `deku` CLI over the trusted Unix socket; only use direct HTTP if you already have a dashboard token.
3. Create or inspect app state.
4. Set required config.
5. Deploy via archive or image path.
6. Poll deployment status and inspect logs.
7. Verify resulting app state.

## Preflight Checks

Recommended checks before an agent attempts a deploy:

```bash
TOKEN="dku_REPLACE_WITH_OPERATOR_TOKEN"
deku apps list
deku apps info <app>
deku config list <app>
curl -H "Authorization: Bearer ${TOKEN}" \
  http://127.0.0.1:2810/api/apps
```

If the app does not exist yet:

```bash
deku apps create <app>
```

## Starter App Catalog

Before scaffolding a new app from nothing, check the local `templates/` directory in this repository.

Common examples:

```bash
deku deploy run agent-demo --path /absolute/path/to/repo/templates/node-express
deku deploy run agent-demo --path /absolute/path/to/repo/templates/nextjs
deku deploy run agent-demo --path /absolute/path/to/repo/templates/django
```

These starters are adapted for Deku:

- They use Deku-supported `dockerfile` or `nixpacks` builders
- They expect Deku-managed services instead of bundled sidecars
- They stay local to the repo so agents can inspect and modify them directly

## CLI Workflow Examples

Create app state:

```bash
deku apps create agent-demo
deku apps info agent-demo
deku config set agent-demo NODE_ENV=production PORT=3000
deku domains add agent-demo demo.local
```

Deploy from source archive path:

```bash
deku deploy run agent-demo --path /absolute/path/to/app
deku deploy list agent-demo
```

Deploy from a pre-built image:

```bash
deku deploy run agent-demo --image nginx:alpine
deku deploy list agent-demo
```

Inspect state after deploy:

```bash
deku logs agent-demo -n 100
deku ps list agent-demo
deku apps info agent-demo
```

## HTTP API Workflow Examples

If an agent needs direct HTTP access, use a dashboard token that was captured during `deku setup` or minted explicitly with `deku dashboard reset-token`.

Example session:

```bash
TOKEN="dku_REPLACE_WITH_OPERATOR_TOKEN"
```

List apps:

```bash
curl -H "Authorization: Bearer ${TOKEN}" \
  http://127.0.0.1:2810/api/apps
```

Create an app:

```bash
curl -X POST \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{"name":"agent-demo"}' \
  http://127.0.0.1:2810/api/apps
```

Set config:

```bash
curl -X POST \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{"key":"NODE_ENV","value":"production"}' \
  http://127.0.0.1:2810/api/apps/agent-demo/config
```

Deploy from a pre-built image:

```bash
curl -X POST \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{"source":"image","image":"nginx:alpine"}' \
  http://127.0.0.1:2810/api/apps/agent-demo/deploy
```

Deploy from a source archive:

```bash
tar -czf /tmp/agent-demo.tar.gz -C /absolute/path/to/app .

curl -X POST \
  -H "Authorization: Bearer ${TOKEN}" \
  -F archive=@/tmp/agent-demo.tar.gz \
  http://127.0.0.1:2810/api/apps/agent-demo/deploy/archive
```

List deployments:

```bash
curl -H "Authorization: Bearer ${TOKEN}" \
  http://127.0.0.1:2810/api/apps/agent-demo/deployments
```

Fetch recent logs:

```bash
curl -H "Authorization: Bearer ${TOKEN}" \
  "http://127.0.0.1:2810/api/apps/agent-demo/logs?n=100"
```

Inspect app state:

```bash
curl -H "Authorization: Bearer ${TOKEN}" \
  http://127.0.0.1:2810/api/apps/agent-demo
```

## When To Use CLI vs API vs SSH

Use the CLI when:

- The agent already has shell access
- The workflow benefits from built-in command ergonomics
- Source archive deploy is needed

Use the HTTP API when:

- The agent needs machine-structured output directly
- The workflow is built around authenticated HTTP calls
- App inspection or control loops are easier through JSON responses

Use SSH `git push` when:

- You explicitly want to exercise the Git remote deploy path
- The workflow is already organized around server-side Git remotes

Do not make SSH `git push` the default path for agents.

## Failure Handling

If the daemon is missing or unreachable:

- Verify `dekud` is running
- Verify you have a valid dashboard token if you are using direct HTTP
- Verify authenticated requests to `/api/apps` succeed

If the app does not exist:

- Create it before attempting config or deploy operations

If a deploy fails:

- Inspect `deku deploy list <app>`
- Inspect `deku logs <app> -n 100`
- Confirm app config and domains are correct

If domains or config do not match expectations:

- Re-read app info and config
- Treat CLI/API output as the source of truth

If a service dependency is missing:

- Verify object store, managed services, or other optional integrations before depending on them
- Treat missing integrations as preconditions, not implicit defaults

## Current Limitations

The current product shape still has important constraints:

- The dashboard is useful, but the agent contract is CLI/API-first
- Dynamic plugin support exists, but it is not the primary first-party integration model
- Installation and packaging are still maturing

## MCP Later

MCP is not required for the current agent workflow because the CLI and daemon HTTP API already cover the target coding-agent workflow.

MCP becomes worthwhile when Deku needs:

- External assistants without shell access
- Richer machine-readable resource discovery
- Structured tool invocation across apps, deploy, config, domains, logs, and deployments

If Deku adds MCP later, the first MCP server should expose only stable platform primitives:

- Apps
- Deploy
- Config
- Domains
- Logs
- Deployments

The first MCP design should not include dynamic plugin surfaces.

Start MCP-later work only after:

- Start only after the CLI/API workflow documented here is stable and verified

## AGENTS.md

````md
# AGENTS

Agent operating guide for Deku.

This file is for coding agents already running in a repo or shell on the same machine as the Deku project or server. It describes what agents should use, what they should avoid, and the safest default deployment workflow.

## Supported Agent Model

- Primary interface: `deku` CLI
- Fallback machine interface: Daemon HTTP API with an operator-provisioned dashboard token
- Preferred deploy path: Archive or image deploy via `deku deploy run`
- Secondary deploy path: `git push` over SSH
- Default workflow: Deploy + inspect

Use the existing CLI and daemon HTTP API to deploy and inspect apps. Do not treat this file as permission for broad unattended platform administration.

## Preferred Workflow

1. Ensure `dekud` is running and reachable.
2. Prefer the local `deku` CLI over the trusted Unix socket; only use direct HTTP if you already have a dashboard token.
3. Inspect or create the target app.
4. Set required config vars.
5. Deploy with `deku deploy run <app> --path <dir>` or `--image <ref>`.
6. Inspect deployment history and logs.
7. Verify app state before moving on.

Prefer deterministic non-interactive commands. Use `git push` only when you explicitly need to exercise the SSH remote path.

## Preflight Checks

Run these before attempting a deploy:

```bash
TOKEN="dku_REPLACE_WITH_OPERATOR_TOKEN"
deku apps list
deku apps info <app>
deku config list <app>
curl -H "Authorization: Bearer ${TOKEN}" \
  http://127.0.0.1:2810/api/apps
```

If the app does not exist, create it first:

```bash
deku apps create <app>
```

## Starter App Catalog

If you need a known-good starter app quickly, check the local `templates/` directory in this repository before building from scratch.

Examples:

```bash
deku deploy run <app> --path /absolute/path/to/repo/templates/node-express
deku deploy run <app> --path /absolute/path/to/repo/templates/nextjs
deku deploy run <app> --path /absolute/path/to/repo/templates/django
```

The templates are adapted for Deku:

- Use Deku-supported `dockerfile` or `nixpacks` builders
- Expect Deku-managed services instead of bundled sidecars
- Stay local to the repo so agents can copy or inspect them directly

## Recommended Commands

Create or inspect app state:

```bash
deku apps create <app>
deku apps info <app>
deku config set <app> KEY=VALUE
deku config list <app>
deku domains add <app> example.test
```

Deploy from source or image:

```bash
deku deploy run <app> --path /absolute/path/to/source
deku deploy run <app> --image nginx:alpine
deku deploy list <app>
deku deploy rollback <app>
```

Inspect runtime state:

```bash
deku logs <app> -n 100
deku ps list <app>
deku apps info <app>
```

## HTTP API Fallback

Use the daemon API directly when a shell agent needs machine-structured output and already has a valid dashboard token:

```bash
TOKEN="dku_REPLACE_WITH_OPERATOR_TOKEN"

curl -H "Authorization: Bearer ${TOKEN}" \
  http://127.0.0.1:2810/api/apps

curl -X POST \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{"name":"agent-demo"}' \
  http://127.0.0.1:2810/api/apps

curl -X POST \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{"key":"NODE_ENV","value":"production"}' \
  http://127.0.0.1:2810/api/apps/agent-demo/config

curl -X POST \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{"source":"image","image":"nginx:alpine"}' \
  http://127.0.0.1:2810/api/apps/agent-demo/deploy

tar -czf /tmp/agent-demo.tar.gz -C /absolute/path/to/app .
curl -X POST \
  -H "Authorization: Bearer ${TOKEN}" \
  -F archive=@/tmp/agent-demo.tar.gz \
  http://127.0.0.1:2810/api/apps/agent-demo/deploy/archive

curl -H "Authorization: Bearer ${TOKEN}" \
  http://127.0.0.1:2810/api/apps/agent-demo/deployments

curl -H "Authorization: Bearer ${TOKEN}" \
  "http://127.0.0.1:2810/api/apps/agent-demo/logs?n=100"

curl -H "Authorization: Bearer ${TOKEN}" \
  http://127.0.0.1:2810/api/apps/agent-demo
```

## Failure Handling

If `dekud` is unavailable:

- Verify the daemon is running
- Verify you have a valid dashboard token if you are using direct HTTP
- Verify authenticated API requests return HTTP 200

If an app does not exist:

- Create it with `deku apps create <app>`

If a deploy fails:

- Inspect `deku deploy list <app>`
- Inspect `deku logs <app> -n 100`
- Confirm config vars, domains, and process scale are correct

If a dependency is missing:

- Object store, managed services, or other optional integrations should be treated as preconditions
- Do not assume they are configured; verify first

## What Not To Do

- Do not treat `git push` as the default agent deployment method
- Do not assume the dynamic plugin runtime is the right integration point for first-party workflows
- Do not attempt broad unattended platform administration
- Do not assume MCP exists today

## Scope Today

Supported today:

- App create / inspect
- Config set / list / unset
- Deploy run / list / rollback
- Domains add / list / remove
- Logs and process inspection

Out of scope today:

- Plugin authoring via dynamic `cdylib`
- MCP implementation
- Broad unattended platform administration

## MCP Later

MCP is not required for the current agent story because the CLI and daemon HTTP API already cover the target coding-agent workflow.

MCP becomes worthwhile when Deku needs:

- External assistants without shell access
- Richer machine-readable resource discovery
- Structured tool calls across apps, deploy, config, domains, logs, and deployments

If MCP is added later, the first server should expose only stable platform primitives:

- Apps
- Deploy
- Config
- Domains
- Logs
- Deployments

Do not include dynamic plugin surfaces in the first MCP design.
````
