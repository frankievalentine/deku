---
title: Environments
description: Deploy one app to production and to preview environments, each with its own hostname and config overrides.
---

Every app has a `production` environment. You can add more, deploy to them by name, and give them
their own config vars. Each environment is served from its own hostname, so a preview build never
takes over production traffic.

## The model

An environment is a named deployment target belonging to one app. Environments share the app's
image, its build, and its app-wide config vars; what differs is:

- **Which deployment is live.** Each environment has its own live deployment, so deploying a preview
  does not retire the production containers.
- **Which containers serve it.** The proxy pools only the running web containers of that
  environment. A preview container is never added to the production upstream.
- **Which hostname it answers on.** Production keeps the app's domains. Every other environment
  gets a generated hostname.
- **Which config var values it receives.** An override shadows the app-wide value inside one
  environment and nowhere else.

## Generated hostnames

Production is served on the domains you add with `deku domains add`. Every other environment is
served on:

```
<app>-<slug>.<global_domain>
```

For an app named `demo` with a `staging` environment and `global_domain = "apps.test"`, that is
`demo-staging.apps.test`. The hostname is derived, not stored, so renaming the app or changing
`global_domain` changes it on the next reconcile.

An app with **no domain of its own** is served at `<app>-production.<global_domain>` as well, so
that production has a stable hostname instead of only the per-deployment URLs that change with
every deploy. Adding a domain switches production back to it and drops the generated hostname.

Two consequences worth knowing:

- Environments need a `global_domain` in the daemon config (`deku setup` prompts for it) to be
  reachable. Without it there is no name to route on, so Deku writes no vhost for them.

  ```toml
  global_domain = "apps.test"
  ```

- Generated hostnames are served over **HTTP** unless [automatic
  certificates](/certificates/) are configured, which obtains one wildcard certificate covering all
  of them. An app's own domains are covered by that app's certificate either way.

All of an app's vhosts live in one `<app>.conf`. The file is per app rather than per hostname
because `<app>-<slug>` has the same shape as an app name: an app called `demo-staging` and the
`staging` environment of `demo` would otherwise collide.

## Deploying to an environment

```bash
deku deploy run demo                          # production
deku deploy run demo --environment staging    # the staging environment
deku deploy rollback demo                     # rolls back within the target's environment
```

The same applies over the API: `environment` is a field on the deploy body, and a query parameter
on the archive upload.

An unknown environment fails the request before the deploy starts, naming the slugs that do exist,
rather than accepting the deploy and failing later.

A rollback re-deploys the environment the deployment it targets belongs to. Rolling back a
production deployment cannot silently deploy into whichever environment happens to hold the newest
deployment.

## Preview URLs

Every retained deployment is reachable at a hostname of its own:

```
<app>-<slug>-<shortid>.<global_domain>
```

Production is spelled out (`<app>-production-...`) like any other environment, even though its
stable hostname is the app's own domains.

`deku deploy run demo --environment staging` prints the environment's URL and the build's own URL.
The build URL serves that exact deployment, so it keeps working after a later deploy replaces it,
until the deployment falls out of the retention window.

### Retention

An environment's recent deployments stay running so their URLs keep working:

```toml
[previews]
keep_deployments = 3   # per environment, counting the live one
```

A deploy keeps the live deployment plus the newest `keep_deployments - 1` before it, and retires
anything older after the `retire` grace period. Set it to `1` to keep only the live deployment, whose
URL then goes dark when the next deploy replaces it.

Each retained deployment holds its own containers, so raising this number costs memory and ports per
environment. Lowering it takes effect on the next deploy.

Hostnames are derived, not stored: a URL resolves only while its containers are retained, and its
vhost is removed when they are retired, so an expired URL stops resolving rather than failing
through the proxy. `deku deploy list <app>` shows the URL for each deployment that is currently
reachable and `-` for the rest.

## Config overrides

An app-wide config var is inherited by every environment:

```bash
deku config set demo GREETING=hello
```

Adding `--environment` writes an override that applies inside that environment only:

```bash
deku config set demo GREETING=preview --environment staging
deku config list demo --environment staging    # shows the effective set, overrides marked
deku config unset demo GREETING --environment staging   # removes the override, keeps the app-wide value
```

`config list` without `--environment` shows the app-wide values. With it, each row reports where its
value came from: an inherited app-wide value or an environment override. Overrides are encrypted at
rest exactly like app-wide values when an encryption key is configured.

Overrides reach the container on the next deploy: config vars are injected when the environment's
deployment starts, so a redeploy is needed for a change to take effect.

## Managing environments

```bash
deku env list demo
deku env create demo Staging --slug staging --branch main
deku env remove demo staging
```

The slug is derived from the name unless you pass `--slug`, and it must be lowercase letters,
digits, and `-`. `--branch` records the git ref the environment tracks; it is metadata today and is
not wired to automatic deploys.

`production` cannot be removed.

## Current limits

- Generated hostnames, including per-deployment URLs, are HTTP-only until a wildcard certificate
  covers them, as described above.
- `git push` deploys to production. Branch-to-environment mapping is not wired up.
- Authentication, maintenance mode, and redirects are app-scoped: they apply to every environment's
  vhost, not per environment.
