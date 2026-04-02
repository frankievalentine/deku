CREATE TABLE IF NOT EXISTS containers (
  id              TEXT PRIMARY KEY,
  app_id          TEXT NOT NULL REFERENCES apps(id),
  deployment_id   TEXT NOT NULL REFERENCES deployments(id),
  process_type    TEXT NOT NULL DEFAULT 'web',
  status          TEXT NOT NULL DEFAULT 'running',
  host_port       INTEGER,
  created_at      DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
