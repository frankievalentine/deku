# Deku Starter Templates

This directory is the local source of truth for Deku starter apps.

These are local starter directories shaped for Deku's own deploy model.

## How To Use A Template

1. Copy or clone the template directory you want to start from.
2. Create the target app in Deku.
3. Deploy the template directory with `deku deploy run`.

Example:

```bash
deku apps create my-app
deku deploy run my-app --path /absolute/path/to/repo/templates/node-express
```

## Catalog

| Template | Builder | Stack | Package Manager |
| --- | --- | --- | --- |
| `django` | `nixpacks` | Django | `pip` |
| `laravel` | `dockerfile` | Laravel | `composer` |
| `nextjs` | `dockerfile` | Next.js | `npm` |
| `nuxt` | `nixpacks` | Nuxt | `bun` |
| `astro` | `dockerfile` | Astro | `bun` |
| `node-express` | `dockerfile` | Node + Express | `npm` |
| `fastapi` | `dockerfile` | FastAPI | `pip` |
| `hono` | `dockerfile` | Hono | `bun` |
| `vite-react` | `dockerfile` | Vite + React | `bun` |

## Maintenance Notes

- Prefer framework-default layouts over platform-specific wiring.
- Use Deku-managed Postgres, Redis, or MySQL instead of bundling sidecars into these template directories.
- Keep `template.json` current with the most recent verification date when refreshing a template.
- Use Bun where the framework supports it cleanly and the builder path stays easy to understand.
