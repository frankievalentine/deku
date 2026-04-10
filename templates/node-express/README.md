# Node Express Starter

This template is a minimal Express API for Deku with a production Dockerfile and a `/health` route.

## Deploy

```bash
deku apps create express-demo
deku deploy run express-demo --path /absolute/path/to/repo/templates/node-express
```

## Runtime Notes

- Health endpoint: `/health`
- Builder: `dockerfile`
- Runtime port: `3000`

## Optional Services

- `deku postgres` for relational data
- `deku redis` for cache or queues
