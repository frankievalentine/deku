---
title: deku.toml
description: Project-level Deku configuration reference.
---

`deku.toml` is the project-level configuration file used during builds and deploys.

## Example

```toml
[build]
builder = "dockerfile"
dockerfile = "Dockerfile"
context = "."

[build.args]
NODE_ENV = "production"

[deploy]
healthcheck = "/health"
port = 3000
wait = 5
timeout = 30
attempts = 5
retire = 60

[processes]
web = 1
worker = 2
```

## Build

- `builder`: `dockerfile | railpack | pack | image | compose | auto`
- `pack_builder`: builder image passed to `pack build --builder` (for example
  `paketobuildpacks/builder-jammy-base`). Optional.

The `pack` builder needs the Buildpacks `pack` CLI on `PATH`. Without `pack_builder`, `pack` uses
the host-wide default set by `pack config default-builder`, and the deploy fails with pack's own
instructions when neither is configured. The built image must also define a process and listen on a
port, or the deploy stops at the health check.
- `dockerfile`: Override Dockerfile path
- `context`: Build context directory
- `build.args`: Build-time environment passed to the builder

Without `builder = "railpack"`, Deku auto-detects a `Dockerfile`, `dockerfile`, or Docker Compose file and uses the matching builder. `railpack` runs its own language detection, so it is only used with an explicit selection. Railpack builds with BuildKit; `dekud` runs a managed BuildKit container (`deku-buildkit`) by default. Override it with `[buildkit]` in the daemon config (`managed`, `image`, `container_name`, or an explicit `host`).

## Deploy

- `healthcheck`: HTTP path used during rollout validation
- `port`: Override the auto-detected web container port
- `wait`: Seconds to wait before health checks start
- `timeout`: Per-attempt health-check timeout
- `attempts`: Number of health-check retries
- `retire`: Seconds before old containers are retired

These settings are applied by the live deploy pipeline in `dekud`, not just documented metadata.

Every running web replica is checked before a deploy switches traffic, and all ready replicas are
added to the proxy upstream pool. A single unhealthy replica fails the deploy and leaves the
previous version serving, and the failure names the replicas that did not become ready. This is why
`deku ps scale <app> web=N` spreads load instead of only adding idle containers.

## Processes

The `[processes]` table controls desired process counts by Procfile type.

Example:

```toml
[processes]
web = 2
worker = 1
```
