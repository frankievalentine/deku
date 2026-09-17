CREATE TABLE IF NOT EXISTS app_deploy_tokens (
    id           TEXT PRIMARY KEY NOT NULL,
    app_id       TEXT NOT NULL REFERENCES apps(id),
    name         TEXT NOT NULL,
    token_hash   TEXT NOT NULL UNIQUE,
    token_prefix TEXT NOT NULL,
    created_at   DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_used_at DATETIME
);

CREATE INDEX IF NOT EXISTS idx_app_deploy_tokens_app_id ON app_deploy_tokens(app_id);
