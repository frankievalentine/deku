# Django Starter

This template is a minimal Django app adapted for Deku with the `nixpacks` builder.

It deploys as a plain Deku app with environment-driven configuration.

## Deploy

```bash
deku apps create django-demo
deku deploy run django-demo --path /absolute/path/to/repo/templates/django
```

## Runtime Notes

- Health endpoint: `/health`
- Default database: SQLite
- Optional managed service: `deku postgres` if you want to switch to Postgres later
- When `DATABASE_URL` is present, the app will try to use it

## Files

- `deku.toml`: Deku builder and deploy settings
- `Procfile`: explicit Nixpacks start command
- `requirements.txt`: Python dependencies
- `config/`: Django project configuration
