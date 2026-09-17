---
title: Deploy tokens
description: Give CI and provider webhooks a credential that can only deploy one app.
---

A deploy token ships releases without handing over your dashboard token. Each token belongs to one app, and it authorizes only the two routes CI needs:

- `POST /api/apps/<app>/deploy`
- `POST /api/apps/<app>/deploy/archive`

Everything else — config, domains, logs, services, other apps — rejects a deploy token. A leaked token is therefore limited to redeploying the app it was minted for, and revoking it does not disturb anything else.

## Mint a token

```bash
deku deploy token create my-app --name github-actions
```

The token is printed once and cannot be retrieved later. Store it as a secret in your CI system.

```bash
deku deploy token list my-app
deku deploy token revoke my-app <id>
```

`list` shows the token id, its name, when it was created, and when it was last used, so you can spot tokens that are idle and revoke them.

## Use a token in CI

Send it as a bearer token:

```bash
curl -X POST \
  -H "Authorization: Bearer $DEKU_DEPLOY_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"source":"image","image":"ghcr.io/acme/my-app:latest"}' \
  "http://deploy.example.com:2810/api/apps/my-app/deploy"
```

Or deploy a source archive:

```bash
tar -czf /tmp/app.tar.gz -C /path/to/source .
curl -X POST \
  -H "Authorization: Bearer $DEKU_DEPLOY_TOKEN" \
  -F archive=@/tmp/app.tar.gz \
  "http://deploy.example.com:2810/api/apps/my-app/deploy/archive"
```

A `?token=…` query parameter is also accepted, which is convenient for provider webhooks that cannot set headers. Prefer the header where you control the request: query strings are more likely to end up in logs and proxy access records.

## What a token cannot do

- Deploy a different app (rejected)
- Roll back (use the dashboard or CLI)
- Read or change config, domains, services, or logs

Deploying with a token still runs the full pipeline: build, release phase, health check, and route swap. A failed deploy leaves the previous version serving.

## Related

- [Runtime access](/runtime-access/) for `deku run` and `deku exec`
- [CLI reference](/reference/cli-reference/) for `deploy token` and `config import`
- [Backups](/backups/) if a release needs a backup first
