use chrono::Utc;
use deku_core::types::Event;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;

pub type EventSender = Arc<EventBus>;

#[derive(Debug, Clone)]
pub struct EventBus {
    tx: broadcast::Sender<Event>,
    pool: SqlitePool,
}

impl EventBus {
    pub fn new(pool: SqlitePool) -> Arc<Self> {
        let (tx, _) = broadcast::channel(1024);
        Arc::new(Self { tx, pool })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }

    pub fn emit(
        &self,
        app_id: Option<String>,
        event_type: impl Into<String>,
        payload: Option<serde_json::Value>,
    ) {
        let event = Event {
            id: Uuid::new_v4().to_string(),
            app_id,
            event_type: event_type.into(),
            payload: payload.map(|p| p.to_string()),
            created_at: Utc::now(),
        };
        let _ = self.tx.send(event.clone());
        let pool = self.pool.clone();
        tokio::spawn(async move {
            if let Err(e) = crate::db::queries::persist_event(&pool, &event).await {
                tracing::error!("failed to persist event: {e}");
            }
        });
    }
}
