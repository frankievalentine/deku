# Next.js Starter

This template is a plain App Router Next.js starter adapted for Deku with a multi-stage Dockerfile and standalone output.

## Deploy

```bash
deku apps create nextjs-demo
deku deploy run nextjs-demo --path /absolute/path/to/repo/templates/nextjs
```

## Runtime Notes

- Health endpoint: `/health`
- Builder: `dockerfile`
- Runtime port: `3000`

## Files

- `Dockerfile`: production multi-stage Next.js image
- `next.config.mjs`: enables standalone output
- `app/`: App Router source
