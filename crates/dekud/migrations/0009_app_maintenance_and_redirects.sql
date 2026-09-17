ALTER TABLE apps ADD COLUMN maintenance INTEGER NOT NULL DEFAULT 0;
ALTER TABLE apps ADD COLUMN maintenance_message TEXT;

CREATE TABLE IF NOT EXISTS redirects (
    id          TEXT PRIMARY KEY,
    app_id      TEXT NOT NULL REFERENCES apps(id),
    source_path TEXT NOT NULL,
    target      TEXT NOT NULL,
    code        INTEGER NOT NULL DEFAULT 302,
    created_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(app_id, source_path)
);
