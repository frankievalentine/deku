-- Browser sessions for the dashboard.
--
-- A browser exchanges the dashboard token for a session once, and from then on
-- sends an HttpOnly cookie that page scripts cannot read. Only a hash of the
-- session id is stored, and each session records which token it came from, so
-- rotating the token ends every session issued under the old one.

CREATE TABLE IF NOT EXISTS dashboard_sessions (
    id_hash     TEXT PRIMARY KEY,
    token_hash  TEXT NOT NULL,
    created_at  DATETIME NOT NULL,
    expires_at  DATETIME NOT NULL
);
