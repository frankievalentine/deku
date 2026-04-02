use deku_core::types::Event;
use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;
use chrono::Utc;

pub type EventSender = Arc<EventBus>;

#[derive(Debug, Clone)]
pub struct EventBus {
    tx: broadcast::Sender<Event>,
}

impl EventBus {
    pub fn new() -> Arc<Self> {
        let (tx, _) = broadcast::channel(1024);
        Arc::new(Self { tx })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }

    pub fn emit(&self, app_id: Option<String>, event_type: impl Into<String>, payload: Option<serde_json::Value>) {
        let event = Event {
            id: Uuid::new_v4().to_string(),
            app_id,
            event_type: event_type.into(),
            payload: payload.map(|p| p.to_string()),
            created_at: Utc::now(),
        };
        // Ignore send errors (no subscribers is fine)
        let _ = self.tx.send(event);
    }
}

impl Default for EventBus {
    fn default() -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self { tx }
    }
}
