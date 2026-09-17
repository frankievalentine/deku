CREATE TABLE IF NOT EXISTS app_auth (
    app_id        TEXT PRIMARY KEY NOT NULL,
    mode          TEXT NOT NULL,
    username      TEXT,
    password_hash TEXT,
    forward_url   TEXT,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);
