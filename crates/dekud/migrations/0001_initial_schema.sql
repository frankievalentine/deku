CREATE TABLE IF NOT EXISTS apps (
  id          TEXT PRIMARY KEY,
  name        TEXT UNIQUE NOT NULL,
  created_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  locked      BOOLEAN NOT NULL DEFAULT FALSE,
  status      TEXT NOT NULL DEFAULT 'created'
);

CREATE TABLE IF NOT EXISTS deployments (
  id          TEXT PRIMARY KEY,
  app_id      TEXT NOT NULL REFERENCES apps(id),
  status      TEXT NOT NULL,
  builder     TEXT NOT NULL,
  image_tag   TEXT,
  created_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  finished_at DATETIME
);

CREATE TABLE IF NOT EXISTS config_vars (
  app_id      TEXT NOT NULL REFERENCES apps(id),
  key         TEXT NOT NULL,
  value       TEXT NOT NULL,
  is_global   BOOLEAN NOT NULL DEFAULT FALSE,
  PRIMARY KEY (app_id, key)
);

CREATE TABLE IF NOT EXISTS domains (
  id          TEXT PRIMARY KEY,
  app_id      TEXT NOT NULL REFERENCES apps(id),
  domain      TEXT NOT NULL UNIQUE,
  created_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS port_mappings (
  id              TEXT PRIMARY KEY,
  app_id          TEXT NOT NULL REFERENCES apps(id),
  host_port       INTEGER NOT NULL,
  container_port  INTEGER NOT NULL,
  protocol        TEXT NOT NULL DEFAULT 'tcp'
);

CREATE TABLE IF NOT EXISTS storage_mounts (
  id              TEXT PRIMARY KEY,
  app_id          TEXT NOT NULL REFERENCES apps(id),
  host_path       TEXT NOT NULL,
  container_path  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS resource_limits (
  app_id        TEXT NOT NULL REFERENCES apps(id),
  process_type  TEXT NOT NULL DEFAULT '_all_',
  cpu           TEXT,
  memory        TEXT,
  memory_swap   TEXT,
  network       TEXT,
  PRIMARY KEY (app_id, process_type)
);

CREATE TABLE IF NOT EXISTS ssh_keys (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL UNIQUE,
  public_key  TEXT NOT NULL,
  created_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS events (
  id          TEXT PRIMARY KEY,
  app_id      TEXT REFERENCES apps(id),
  event_type  TEXT NOT NULL,
  payload     TEXT,
  created_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS process_scale (
  app_id        TEXT NOT NULL REFERENCES apps(id),
  process_type  TEXT NOT NULL,
  count         INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (app_id, process_type)
);

CREATE TABLE IF NOT EXISTS docker_options (
  id          TEXT PRIMARY KEY,
  app_id      TEXT NOT NULL REFERENCES apps(id),
  phase       TEXT NOT NULL,
  option      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS networks (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL UNIQUE,
  created_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS app_networks (
  app_id        TEXT NOT NULL REFERENCES apps(id),
  network_id    TEXT NOT NULL REFERENCES networks(id),
  attach_phase  TEXT NOT NULL,
  PRIMARY KEY (app_id, network_id, attach_phase)
);
