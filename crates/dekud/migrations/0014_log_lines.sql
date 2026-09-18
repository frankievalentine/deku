-- Log lines for every deployment, searchable.
--
-- Build output and runtime container output land here with the deployment and
-- environment they belong to, so a past deployment's logs stay readable after
-- its containers are retired. Search uses FTS5, which the bundled SQLite
-- compiles in (see libsqlite3-sys bundled flags), so this needs no external
-- log service.

CREATE TABLE IF NOT EXISTS log_lines (
    id            TEXT PRIMARY KEY,
    app_id        TEXT NOT NULL REFERENCES apps(id),
    deployment_id TEXT,
    environment_id TEXT,
    source        TEXT NOT NULL,
    stream        TEXT NOT NULL,
    level         TEXT NOT NULL DEFAULT 'INFO',
    message       TEXT NOT NULL,
    created_at    DATETIME NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_log_lines_app_time ON log_lines (app_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_log_lines_deployment ON log_lines (deployment_id);

-- Content-carrying FTS5 index: the message text lives here too, which is what
-- lets search return snippet() and highlight() for the UI.
CREATE VIRTUAL TABLE IF NOT EXISTS log_lines_fts USING fts5(message);

-- Keep the index in step regardless of which path writes or prunes a row.
CREATE TRIGGER IF NOT EXISTS log_lines_after_insert AFTER INSERT ON log_lines BEGIN
    INSERT INTO log_lines_fts (rowid, message) VALUES (new.rowid, new.message);
END;

CREATE TRIGGER IF NOT EXISTS log_lines_after_delete AFTER DELETE ON log_lines BEGIN
    DELETE FROM log_lines_fts WHERE rowid = old.rowid;
END;
