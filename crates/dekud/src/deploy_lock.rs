use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::Mutex as AsyncMutex;

#[derive(Default)]
pub struct AppDeployLocks {
    inner: Mutex<HashMap<String, Arc<AsyncMutex<()>>>>,
}

impl AppDeployLocks {
    pub fn for_app(&self, app_id: &str) -> Arc<AsyncMutex<()>> {
        let mut map = match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        map.entry(app_id.to_string())
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::AppDeployLocks;
    use std::sync::Arc;

    #[tokio::test]
    async fn serializes_same_app() {
        let locks = AppDeployLocks::default();
        let first = locks.for_app("app-a");
        let second = locks.for_app("app-a");

        let guard = first.lock().await;
        assert!(second.try_lock().is_err(), "second acquire must wait");
        drop(guard);
        assert!(second.try_lock().is_ok(), "released guard frees the app");
    }

    #[tokio::test]
    async fn allows_different_apps_concurrently() {
        let locks = AppDeployLocks::default();
        let app_a = locks.for_app("app-a");
        let app_b = locks.for_app("app-b");

        let guard_a = app_a.lock().await;
        assert!(app_b.try_lock().is_ok(), "different app must not block");
        drop(guard_a);
    }

    #[tokio::test]
    async fn returns_same_lock_for_same_app() {
        let locks = AppDeployLocks::default();
        let first = locks.for_app("app-a");
        let second = locks.for_app("app-a");
        assert!(Arc::ptr_eq(&first, &second));
    }
}
