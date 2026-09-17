-- Environments: a named deployment target within an app.
--
-- Every app has a `production` environment. `deku deploy run <app>` targets it,
-- so existing apps and commands are unaffected; `--environment <slug>` targets
-- another one. An environment records the git ref it tracks for display and for
-- picking a target, but nothing auto-deploys on push yet.

CREATE TABLE IF NOT EXISTS environments (
    id            TEXT PRIMARY KEY,
    app_id        TEXT NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    slug          TEXT NOT NULL,
    branch        TEXT,
    is_production BOOLEAN NOT NULL DEFAULT FALSE,
    created_at    DATETIME NOT NULL,
    UNIQUE (app_id, slug)
);

-- Backfill: every existing app becomes a production environment, so all current
-- deployments and config vars have somewhere to belong.
INSERT INTO environments (id, app_id, name, slug, branch, is_production, created_at)
SELECT lower(hex(randomblob(16))), id, 'production', 'production', NULL, 1, CURRENT_TIMESTAMP
FROM apps;

-- config_vars needs a rebuild, not an ALTER: the original table has
-- PRIMARY KEY (app_id, key), which forbids a second value for the same key in a
-- different environment. The new uniqueness is expressed with two partial indexes
-- so an app-wide row (environment_id IS NULL) and per-environment overrides can
-- coexist.
CREATE TABLE config_vars_new (
    app_id         TEXT NOT NULL REFERENCES apps(id),
    environment_id TEXT REFERENCES environments(id) ON DELETE CASCADE,
    key            TEXT NOT NULL,
    value          TEXT NOT NULL,
    is_global      BOOLEAN NOT NULL DEFAULT FALSE
);

INSERT INTO config_vars_new (app_id, environment_id, key, value, is_global)
SELECT app_id, NULL, key, value, is_global FROM config_vars;

DROP TABLE config_vars;

ALTER TABLE config_vars_new RENAME TO config_vars;

CREATE UNIQUE INDEX IF NOT EXISTS idx_config_vars_app_wide
    ON config_vars (app_id, key) WHERE environment_id IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_config_vars_environment
    ON config_vars (app_id, environment_id, key) WHERE environment_id IS NOT NULL;

-- Deployments belong to the environment they were deployed to.
ALTER TABLE deployments ADD COLUMN environment_id TEXT REFERENCES environments(id);

UPDATE deployments SET environment_id = (
    SELECT e.id FROM environments e
    WHERE e.app_id = deployments.app_id AND e.is_production = 1
);
