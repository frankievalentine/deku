# FastAPI Starter

This template is a minimal FastAPI app for Deku with a Dockerfile build and environment-driven configuration.

## Deploy

```bash
deku apps create fastapi-demo
deku deploy run fastapi-demo --path /absolute/path/to/repo/templates/fastapi
```

## Runtime Notes

- Health endpoint: `/health`
- Builder: `dockerfile`
- Runtime port: `8000`
- Optional managed service: `deku postgres`

## Environment

- `DATABASE_URL`: optional connection string for a managed database
