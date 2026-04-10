---
title: App Templates
description: Local starter app directories for Deku and how to choose the right one.
---

Deku ships a local starter catalog in the repository under `templates/`.

Each starter is a local source directory shaped for Deku's own deploy model:

- they use Deku-supported `dockerfile` or `nixpacks` builders
- they expect Deku-managed services instead of bundled sidecars
- they stay small enough to inspect, copy, and modify directly
- Bun is used in the starters where it fits naturally without making the deploy path harder to reason about

If you already cloned the repository, copy the directory you want from `templates/<name>`. If you are browsing the public docs, use the GitHub links below.

## Quick Picks

- Use `templates/node-express` when you want the smallest API starter.
- Use `templates/hono` when you want the smallest Bun-native API starter.
- Use `templates/nextjs` or `templates/nuxt` when you want an SSR-capable JavaScript app.
- Use `templates/astro` when you want a static site served from a container.
- Use `templates/vite-react` when you want a Bun-built frontend SPA.
- Use `templates/django`, `templates/laravel`, or `templates/fastapi` when you want a framework starter that can grow into a database-backed app.

## Dockerfile Starters

### Next.js

Plain App Router starter with standalone output.

- Repo directory: [templates/nextjs](https://github.com/frankievalentine/deku/tree/main/templates/nextjs)
- Builder: `dockerfile`
- Package manager: `npm`
- Best for: full-stack React apps and SSR routes
- Health check: `/health`
- Pair with: managed services through framework-standard env vars

```bash
deku deploy run nextjs-demo --path /absolute/path/to/repo/templates/nextjs
```

### Laravel

Laravel starter with a lightweight local overlay and a Docker build that creates a fresh framework skeleton.

- Repo directory: [templates/laravel](https://github.com/frankievalentine/deku/tree/main/templates/laravel)
- Builder: `dockerfile`
- Package manager: `composer` at build time
- Best for: PHP web apps and dashboard-style applications
- Health check: `/health`
- Pair with: `deku postgres`, `deku mysql`, or `deku redis`

```bash
deku deploy run laravel-demo --path /absolute/path/to/repo/templates/laravel
```

### Astro

Static Astro site served from a production container.

- Repo directory: [templates/astro](https://github.com/frankievalentine/deku/tree/main/templates/astro)
- Builder: `dockerfile`
- Package manager: `bun`
- Best for: docs, marketing sites, and content-heavy frontends
- Health check: `/health/`
- Pair with: no backing service by default

```bash
deku deploy run astro-demo --path /absolute/path/to/repo/templates/astro
```

### Node Express

Minimal JSON API starter with a small production image and `/health` route.

- Repo directory: [templates/node-express](https://github.com/frankievalentine/deku/tree/main/templates/node-express)
- Builder: `dockerfile`
- Package manager: `npm`
- Best for: APIs, webhooks, and service backends
- Health check: `/health`
- Pair with: `deku postgres` or `deku redis`

```bash
deku deploy run express-demo --path /absolute/path/to/repo/templates/node-express
```

### FastAPI

Minimal FastAPI app with environment-driven configuration and a production container.

- Repo directory: [templates/fastapi](https://github.com/frankievalentine/deku/tree/main/templates/fastapi)
- Builder: `dockerfile`
- Package manager: `pip`
- Best for: Python APIs and service backends
- Health check: `/health`
- Pair with: `deku postgres`

```bash
deku deploy run fastapi-demo --path /absolute/path/to/repo/templates/fastapi
```

### Hono

Small Bun-native API starter with a minimal Hono server and health route.

- Repo directory: [templates/hono](https://github.com/frankievalentine/deku/tree/main/templates/hono)
- Builder: `dockerfile`
- Package manager: `bun`
- Best for: lightweight APIs and webhook services
- Health check: `/health`
- Pair with: `deku postgres` or `deku redis`

```bash
deku deploy run hono-demo --path /absolute/path/to/repo/templates/hono
```

### Vite React

React single-page app built with Vite and Bun, then served from Nginx.

- Repo directory: [templates/vite-react](https://github.com/frankievalentine/deku/tree/main/templates/vite-react)
- Builder: `dockerfile`
- Package manager: `bun`
- Best for: frontend-only apps and dashboards backed by another API
- Health check: `/health`
- Pair with: another Deku app or external API

```bash
deku deploy run vite-demo --path /absolute/path/to/repo/templates/vite-react
```

## Nixpacks Starters

### Django

Django starter that defaults to SQLite for the first deploy and can switch to a managed database later.

- Repo directory: [templates/django](https://github.com/frankievalentine/deku/tree/main/templates/django)
- Builder: `nixpacks`
- Package manager: `pip`
- Best for: traditional server-rendered apps and admin-heavy backends
- Health check: `/health`
- Pair with: `deku postgres` when you outgrow SQLite

```bash
deku deploy run django-demo --path /absolute/path/to/repo/templates/django
```

### Nuxt

Clean SSR-capable Nuxt starter with an explicit production start command.

- Repo directory: [templates/nuxt](https://github.com/frankievalentine/deku/tree/main/templates/nuxt)
- Builder: `nixpacks`
- Package manager: `bun`
- Best for: Vue SSR apps and content-rich product applications
- Health check: `/api/health`
- Pair with: managed services through runtime config and env vars

```bash
deku deploy run nuxt-demo --path /absolute/path/to/repo/templates/nuxt
```

## Managed Service Pairings

- Django: default SQLite; pair with `deku postgres` when you want a managed relational database
- Laravel: pair with `deku postgres`, `deku mysql`, or `deku redis` as the app grows
- FastAPI: pair with `deku postgres` when you need persistent data
- Hono: pair with `deku postgres` or `deku redis` when you need persistence
- Node Express: pair with `deku postgres` or `deku redis` for stateful workloads
- Next.js and Nuxt: add Deku-managed services through framework-standard environment variables as needed
- Vite React: static by default; pair it with another Deku app when you need an API
- Astro: static by default; no backing service required

## Selection Notes

- The Laravel starter intentionally keeps a lightweight local overlay and bootstraps the official Laravel skeleton during the Docker build.
- These templates do not bundle sidecars. Use Deku-managed services instead.
- Bun is enabled in the templates where it improves install and build speed without adding runtime ambiguity: `astro`, `nuxt`, `hono`, and `vite-react`.

## Typical Flow

```bash
deku apps create my-app
deku deploy run my-app --path /absolute/path/to/repo/templates/node-express
deku deploy list my-app
deku logs my-app -n 100
```
