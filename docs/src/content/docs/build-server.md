---
title: Build server
description: Offload image builds to a remote SSH host and transfer the result through a registry.
---

By default Deku builds images on the same host that runs your apps, so a heavy build competes with
production for CPU and memory. A **build server** moves that work to a dedicated machine: the source
is shipped over SSH, the image is built there with the builder you would use locally, pushed to a
registry, and pulled back on the deploy host.

The control plane still deploys to one host. Only the build is offloaded.

## Why a registry is required

Railpack (the general-purpose builder) always pipes its BuildKit output into `docker load` on the
machine running the CLI, and has no flag to push to a registry. Offloading therefore means running
the builder *on the build host*. Because the build host is not the deploy host, the image has to
travel between them, and a registry is the only sane way to do that.

Deku pulls the immutable tag on the deploy host and retags it locally, so container creation,
retirement, and rollback behave exactly as they do for a local build.

## Setup

Configure a registry and a build host. The password is stored in `config.toml` (mode `0600`) and is
redacted everywhere it is displayed.

```bash
deku registry setup --server ghcr.io/acme --username acme
# prompts for a password or scoped token; use --password for automation

deku build-host setup --host ssh://deku@builder.internal
# optional: --identity-file ~/.deku/build_key
# optional: --name builder (the value accepted by --build-host)
# optional: --buildkit-host docker-container://deku-buildkit
```

Verify the remote toolchain before the first deploy:

```bash
deku build-host init    # creates or starts a managed BuildKit container on the builder
deku build-host check   # probes ssh, docker, railpack, and BuildKit
```

`check` exits non-zero when a check fails, so it is safe to gate a pipeline on it.

The build host needs SSH access for a non-root user, Docker, and railpack. It does **not** run your
apps, and it should be treated as untrusted build compute: keep it distinct from production.

## Deploying

Once a build host is configured, source deploys use it automatically:

```bash
deku deploy run my-app --path /srv/my-app
```

To build on the deploy host for a single deploy, override it:

```bash
deku deploy run my-app --path /srv/my-app --build-host local
```

Build output streams live, prefixed with the remote builder's own log lines, followed by the pull on
the deploy host and the normal health check.

## How a remote build runs

1. The source directory is packed into a `tar.gz` and streamed over SSH into a fresh `0700` temporary
   directory on the build host.
2. The builder runs there: `railpack build`, `docker build` (Dockerfile or Compose), or `pack build`.
3. The image is tagged `registry/namespace/app:deploy-id` and pushed. If credentials are configured,
   `docker login` runs first with the password supplied on stdin, never in the command line.
4. The deploy host pulls that reference, retags it to the local immutable tag, and continues with the
   existing deploy pipeline.
5. The remote workspace is always removed, including when the build fails.

Values taken from your repository (`deku.toml`, Compose files) are shell-quoted, and build contexts
that escape the source directory are rejected before anything is shipped.

## Failure behavior

A build host outage, a failed remote build, or a failed push fails the deploy. The previous version
keeps serving, and the deployment is recorded as `failed`. The registry is left untouched, so a
partial build never becomes a deployable tag.

## Configuration reference

`config.toml`:

```toml
[registry]
server    = "ghcr.io/acme"   # registry host plus optional owner path
username  = "acme"
password  = "..."            # or a scoped token
namespace = "deku"           # produces ghcr.io/acme/deku/<app>:<deploy-id>

[build_host]
name          = "builder"
host          = "ssh://deku@builder.internal"
identity_file = "/root/.deku/build_key"                 # optional
buildkit_host = "docker-container://deku-buildkit"      # railpack's BuildKit
```

Commands and endpoints:

| Action | CLI | API |
| --- | --- | --- |
| Show status | `deku build-host info` | `GET /api/build-host` |
| Configure | `deku build-host setup --host …` | `POST /api/build-host` |
| Remove | `deku build-host unset` | `DELETE /api/build-host` |
| Probe the host | `deku build-host check` | `POST /api/build-host/check` |
| Prepare BuildKit | `deku build-host init` | `POST /api/build-host/init` |
| Show registry | `deku registry info` | `GET /api/registry` |
| Configure registry | `deku registry setup --server …` | `POST /api/registry` |
| Remove registry | `deku registry unset` | `DELETE /api/registry` |

Config changes take effect on the next deploy; no daemon restart is needed.

## Scope and limitations

- Supported builders: railpack, Dockerfile, Compose, and pack. Pre-built image deploys never use a
  build host.
- Source transfer is `tar` over SSH. Git-clone-on-builder is not implemented.
- One registry and one build host are supported.
- BuildKit cache lives on the build host; shared or registry-backed cache is not configured for you.
