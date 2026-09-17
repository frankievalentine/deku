CREATE TABLE IF NOT EXISTS backup_schedules (
    service_id     TEXT PRIMARY KEY NOT NULL,
    interval_hours INTEGER NOT NULL,
    retention      INTEGER NOT NULL,
    enabled        INTEGER NOT NULL DEFAULT 1,
    last_run_at    DATETIME,
    last_status    TEXT,
    next_run_at    DATETIME,
    created_at     DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at     DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
