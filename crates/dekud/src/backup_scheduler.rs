//! Runs service backup schedules: creates the backup, prunes old backups to
//! the configured retention, and advances the next run time.

use std::time::Duration;

use anyhow::Result;
use chrono::{Duration as ChronoDuration, Utc};
use tokio::time::interval;
use tracing::{info, warn};

use crate::api::SharedState;
use crate::db::queries;
use crate::services::database;

/// Spawn the background scheduler loop.
pub fn spawn(state: SharedState) {
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(60));
        loop {
            ticker.tick().await;
            if let Err(error) = run_due(&state).await {
                warn!(error = %error, "backup scheduler tick failed");
            }
        }
    });
}

async fn run_due(state: &SharedState) -> Result<()> {
    let due = queries::list_due_backup_schedules(&state.pool, Utc::now()).await?;

    for schedule in due {
        let service = match queries::get_service_by_id(&state.pool, &schedule.service_id).await {
            Ok(service) => service,
            Err(error) => {
                warn!(
                    service_id = %schedule.service_id,
                    error = %error,
                    "backup schedule references a missing service; removing it"
                );
                let _ = queries::delete_backup_schedule(&state.pool, &schedule.service_id).await;
                continue;
            }
        };

        let result = database::backup_for_plugin(
            &state.pool,
            &state.docker,
            &state.config,
            &service.plugin,
            &service.name,
        )
        .await;

        let status = match result {
            Ok(backup) => {
                info!(
                    service = %service.name,
                    backup = %backup.id,
                    size_bytes = backup.size_bytes,
                    "scheduled backup created"
                );
                if let Some(object_store) = state.config.object_store.as_ref() {
                    prune(state, &service.id, schedule.retention, object_store).await;
                }
                "ok"
            }
            Err(error) => {
                warn!(service = %service.name, error = %error, "scheduled backup failed");
                "error"
            }
        };

        let next_run = Utc::now() + ChronoDuration::hours(schedule.interval_hours.max(1));
        queries::mark_backup_schedule_run(&state.pool, &service.id, status, next_run).await?;
    }

    Ok(())
}

async fn prune(
    state: &SharedState,
    service_id: &str,
    retention: i64,
    object_store: &deku_core::types::ObjectStoreConfig,
) {
    let keep = retention.max(1);
    let stale = match queries::backups_beyond_retention(&state.pool, service_id, keep).await {
        Ok(stale) => stale,
        Err(error) => {
            warn!(error = %error, "failed to list backups for retention pruning");
            return;
        }
    };

    for backup in stale {
        if let Err(error) =
            crate::objectstore::delete_object(object_store, &backup.object_key).await
        {
            warn!(key = %backup.object_key, error = %error, "failed to delete pruned backup object");
        }
        if let Err(error) = queries::delete_service_backup(&state.pool, &backup.id).await {
            warn!(id = %backup.id, error = %error, "failed to delete pruned backup row");
        }
    }
}
