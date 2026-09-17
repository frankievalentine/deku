CREATE TABLE IF NOT EXISTS alerts (
    id            TEXT PRIMARY KEY,
    rule          TEXT NOT NULL,
    severity      TEXT NOT NULL,
    scope         TEXT NOT NULL,
    subject       TEXT NOT NULL,
    message       TEXT NOT NULL,
    first_seen_at DATETIME NOT NULL,
    last_seen_at  DATETIME NOT NULL,
    resolved_at   DATETIME
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_alerts_active_scope
    ON alerts (rule, scope) WHERE resolved_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_alerts_resolved_at ON alerts (resolved_at);
