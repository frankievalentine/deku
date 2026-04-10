# Laravel Starter

This template adapts Laravel to Deku as a Dockerfile-driven app.

To keep the template lightweight and current, the Docker build creates a fresh Laravel skeleton and then overlays the local route and view files checked into this directory.

## Deploy

```bash
deku apps create laravel-demo
deku deploy run laravel-demo --path /absolute/path/to/repo/templates/laravel
```

## Runtime Notes

- Health endpoint: `/health`
- Builder: `dockerfile`
- Runtime port: `8000`
- Optional managed services: `deku postgres`, `deku mysql`, `deku redis`

## Template Shape

- `Dockerfile`: bootstraps the official Laravel skeleton during the image build
- `overrides/routes/web.php`: local routes layered on top of the generated skeleton
- `overrides/resources/views/welcome.blade.php`: local landing page
