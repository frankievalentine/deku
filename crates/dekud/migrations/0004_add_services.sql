CREATE TABLE IF NOT EXISTS services (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL UNIQUE,
    plugin       TEXT NOT NULL,
    container_id TEXT,
    status       TEXT NOT NULL DEFAULT 'stopped',
    config       TEXT NOT NULL DEFAULT '{}',
    created_at   DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS service_links (
    id         TEXT PRIMARY KEY,
    service_id TEXT NOT NULL REFERENCES services(id) ON DELETE CASCADE,
    app_id     TEXT NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
    env_key    TEXT NOT NULL,
    UNIQUE(service_id, app_id)
);

CREATE TABLE IF NOT EXISTS cron_entries (
    id           TEXT PRIMARY KEY,
    app_id       TEXT NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
    schedule     TEXT NOT NULL,
    command      TEXT NOT NULL,
    created_at   DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

ALTER TABLE apps ADD COLUMN tls_enabled INTEGER NOT NULL DEFAULT 0;
