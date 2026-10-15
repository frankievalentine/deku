-- Certificate state for each app domain.
--
-- `mode` is what the operator asked for: `auto` has Angie request a certificate
-- over HTTP-01 once `[acme] custom_domains` is on, `manual` serves the files the
-- operator installed at /etc/angie/ssl/deku_<app>.{crt,key}, and `off` serves
-- the domain over HTTP. `state` is where an `auto` domain is in getting one.
--
-- Kept beside `domains` rather than in it so a domain row stays what it was;
-- a missing row reads as the defaults.

CREATE TABLE IF NOT EXISTS domain_tls (
    domain_id     TEXT PRIMARY KEY REFERENCES domains(id) ON DELETE CASCADE,
    mode          TEXT NOT NULL DEFAULT 'auto'
                  CHECK (mode IN ('auto', 'manual', 'off')),
    state         TEXT NOT NULL DEFAULT 'pending'
                  CHECK (state IN ('pending', 'checking', 'issuing', 'active', 'failed', 'manual', 'off')),
    details       TEXT,
    failures      INTEGER NOT NULL DEFAULT 0,
    expires_at    DATETIME,
    checked_at    DATETIME,
    next_retry_at DATETIME,
    -- Angie's `next_run` when Deku last counted a failure, so one failed
    -- attempt is counted once however many times it is observed.
    observed_run  TEXT,
    updated_at    DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- An app that already serves HTTPS does it with an installed certificate, so
-- its domains keep doing exactly that. Every other domain waits in `auto` and
-- nothing is requested until the operator turns custom domain certificates on.
INSERT OR IGNORE INTO domain_tls (domain_id, mode, state)
SELECT d.id,
       CASE WHEN a.tls_enabled THEN 'manual' ELSE 'auto' END,
       CASE WHEN a.tls_enabled THEN 'manual' ELSE 'pending' END
FROM domains d JOIN apps a ON a.id = d.app_id;
