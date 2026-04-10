# Astro Starter

This template is a simple static Astro site served from a production container.

It uses Bun for dependency installation and the build step.

## Deploy

```bash
deku apps create astro-demo
deku deploy run astro-demo --path /absolute/path/to/repo/templates/astro
```

## Runtime Notes

- Health endpoint: `/health/`
- Builder: `dockerfile`
- Package manager: `bun`
- Runtime port: `80`

## Files

- `Dockerfile`: builds the Astro site and serves it with Nginx
- `src/pages/`: static Astro pages
