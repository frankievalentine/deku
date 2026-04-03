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

- `builder`: `dockerfile | nixpacks | pack | image | auto`
- `dockerfile`: override Dockerfile path
- `context`: build context directory
- `build.args`: build-time environment passed to the builder

## Deploy

- `healthcheck`: HTTP path used during rollout validation
- `port`: override the auto-detected web container port
- `wait`: seconds to wait before health checks start
- `timeout`: per-attempt health-check timeout
- `attempts`: number of health-check retries
- `retire`: seconds before old containers are retired

These settings are applied by the live deploy pipeline in `dekud`, not just documented metadata.

## Processes

The `[processes]` table controls desired process counts by Procfile type.

Example:

```toml
[processes]
web = 2
worker = 1
```
