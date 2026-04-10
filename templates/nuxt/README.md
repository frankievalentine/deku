# Nuxt Starter

This template is a clean SSR-capable Nuxt starter for Deku using `nixpacks`.

It keeps a framework-default app shape with an explicit production start command.
The template includes a Bun lockfile so Nixpacks can build it with Bun.

## Deploy

```bash
deku apps create nuxt-demo
deku deploy run nuxt-demo --path /absolute/path/to/repo/templates/nuxt
```

## Runtime Notes

- Health endpoint: `/api/health`
- Builder: `nixpacks`
- Package manager: `bun`
- Runtime port: `3000`

## Files

- `package.json`: build and start scripts
- `pages/`: app pages
- `server/api/health.get.ts`: health probe route
