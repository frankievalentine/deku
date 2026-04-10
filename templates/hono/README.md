# Hono Starter

This template is a small Hono API for Deku with Bun for both installation and runtime.

## Deploy

```bash
deku apps create hono-demo
deku deploy run hono-demo --path /absolute/path/to/repo/templates/hono
```

## Runtime Notes

- Health endpoint: `/health`
- Builder: `dockerfile`
- Package manager: `bun`
- Runtime port: `3000`

## Files

- `Dockerfile`: Bun-based production image
- `src/index.ts`: Hono app entrypoint
