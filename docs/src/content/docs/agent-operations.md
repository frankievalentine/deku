---
title: Agent Operations
description: How coding agents should deploy and inspect apps with Deku today.
---

This guide describes the current **agent integration v1** story for Deku.

It is aimed at **coding agents already operating in a repo or shell**. The supported model today is intentionally narrow: agents should use the existing CLI and daemon HTTP API to **deploy and inspect apps**, not to perform full autonomous platform administration.

## Supported Agent Contract

Current defaults:

- Primary interface: `deku` CLI
- Fallback machine interface: daemon HTTP API with `Authorization: Bearer $(cat ~/.deku/cli-token)`
- Preferred deploy method: archive or image deploy via CLI or HTTP API
- Secondary deploy method: `git push` over SSH
- Supported operator model: coding agents running in a shell or repo
- Supported workflow class: deploy + inspect

Current v1 scope:

- app create / inspect
- config set / list / unset
- deploy run / list / rollback
- domains add / list / remove
- logs and process/status inspection

Current v1 non-goals:

- full autonomous platform administration
- dynamic plugin authoring
- MCP implementation

## Why CLI + API First

The CLI and daemon API already cover the core workflow agents need today.

For most agent-driven deployments, archive and image deploys are a better fit than `git push` because they are:

- easier to script deterministically
- easier to reason about from an agent loop
- easier to inspect and retry

SSH `git push` deploys remain supported, but they are not the recommended default path for agents in v1.

## Canonical Agent Workflow

The standard agent flow is:

1. Ensure `dekud` is reachable.
2. Read `~/.deku/cli-token`.
3. Create or inspect app state.
4. Set required config.
5. Deploy via archive or image path.
6. Poll deployment status and inspect logs.
7. Verify resulting app state.

## Preflight Checks

Recommended checks before an agent attempts a deploy:

```bash
deku apps list
deku apps info <app>
deku config list <app>
curl -H "Authorization: Bearer $(cat ~/.deku/cli-token)" \
  http://127.0.0.1:2810/api/apps
```

If the app does not exist yet:

```bash
deku apps create <app>
```

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

The daemon writes the long-lived local token to:

```bash
~/.deku/cli-token
```

Example session:

```bash
TOKEN="$(cat ~/.deku/cli-token)"
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

- the agent already has shell access
- the workflow benefits from built-in command ergonomics
- source archive deploy is needed

Use the HTTP API when:

- the agent needs machine-structured output directly
- the workflow is built around authenticated HTTP calls
- app inspection or control loops are easier through JSON responses

Use SSH `git push` when:

- you explicitly want to exercise the Git remote deploy path
- the workflow is already organized around server-side Git remotes

Do not make SSH `git push` the default path for agents in v1.

## Failure Handling

If the daemon is missing or unreachable:

- verify `dekud` is running
- verify `~/.deku/cli-token` exists
- verify authenticated requests to `/api/apps` succeed

If the app does not exist:

- create it before attempting config or deploy operations

If a deploy fails:

- inspect `deku deploy list <app>`
- inspect `deku logs <app> -n 100`
- confirm app config and domains are correct

If domains or config do not match expectations:

- re-read app info and config
- treat CLI/API output as the source of truth

If a service dependency is missing:

- verify object store, managed services, or other optional integrations before depending on them
- treat missing integrations as preconditions, not implicit defaults

## Current Limitations

The current milestone state still has important constraints:

- the dashboard is useful, but the agent contract is CLI/API-first
- dynamic plugin support exists, but it is not the primary first-party integration model
- installation and packaging are still maturing

## MCP Next

MCP is **not required** for the current v1 agent integration because the CLI and daemon HTTP API already cover the target coding-agent workflow.

MCP becomes worthwhile when Deku needs:

- external assistants without shell access
- richer machine-readable resource discovery
- structured tool invocation across apps, deploy, config, domains, logs, and deployments

If Deku adds MCP next, the first MCP server should expose only stable platform primitives:

- apps
- deploy
- config
- domains
- logs
- deployments

The first MCP design should **not** include dynamic plugin surfaces.

The success condition for MCP-next work is:

- start only after the CLI/API workflow documented here is stable and verified
