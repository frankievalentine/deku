# AGENTS

Agent operating guide for Deku.

This file is for coding agents already running in a repo or shell on the same machine as the Deku project or server. It describes what agents should use, what they should avoid, and the safest default deployment workflow.

## Supported Agent Model

- Primary interface: `deku` CLI
- Fallback machine interface: daemon HTTP API with an operator-provisioned dashboard token
- Preferred deploy path: archive or image deploy via `deku deploy run`
- Secondary deploy path: `git push` over SSH
- Default workflow: deploy + inspect

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
