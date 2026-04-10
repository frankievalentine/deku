# Vite React Starter

This template is a React single-page app built with Vite and Bun, then served from Nginx.

## Deploy

```bash
deku apps create vite-demo
deku deploy run vite-demo --path /absolute/path/to/repo/templates/vite-react
```

## Runtime Notes

- Health endpoint: `/health`
- Builder: `dockerfile`
- Package manager: `bun`
- Runtime port: `80`

## Files

- `Dockerfile`: Bun build stage plus Nginx runtime
- `src/`: React app source
- `public/health`: health probe file
