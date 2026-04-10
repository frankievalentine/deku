# EmDash Blog Starter

This template packages the `emdash-simple` Astro app for Deku.

It uses Bun for dependency installation and builds, Node.js for the runtime server, and SQLite plus local uploads for content storage.

## Deploy

```bash
deku apps create emdash-demo
deku storage ensure-directory emdash-demo /var/lib/deku/emdash-demo
deku storage mount emdash-demo /var/lib/deku/emdash-demo /data/emdash
deku deploy run emdash-demo --path /absolute/path/to/repo/templates/emdash-blog
```

## Runtime Notes

- Health endpoint: `/health`
- Builder: `dockerfile`
- Package manager: `bun`
- Runtime port: `3000`
- Persistent path inside the container: `/data/emdash`
- First boot seeds content automatically when `data.db` does not exist

## Operational Notes

- Keep this app on a single replica while it uses SQLite.
- The first deployment should mount persistent storage before traffic.
- If you need horizontal scaling, switch EmDash to Postgres and move media to object storage.

## Files

- `Dockerfile`: Bun build stages and Node runtime image
- `docker-entrypoint.sh`: first-boot SQLite initialization and seed logic
- `deku.toml`: Deku build and healthcheck settings
- `astro.config.mjs`: EmDash configuration using a data directory baked into the image build
