#![allow(improper_ctypes_definitions, unsafe_code)]

use async_trait::async_trait;
use deku_plugin_sdk::error::Result;
use deku_plugin_sdk::{context::AppContext, hooks::AppDestroyHook, PluginDescriptor};

pub struct RedisPlugin;

impl PluginDescriptor for RedisPlugin {
    fn name(&self) -> &'static str {
        "redis"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn app_destroy(&self) -> Option<Box<dyn AppDestroyHook>> {
        Some(Box::new(RdDestroyHook))
    }
}

struct RdDestroyHook;

#[async_trait]
impl AppDestroyHook for RdDestroyHook {
    async fn app_destroy(&self, ctx: &AppContext) -> Result<()> {
        tracing::info!(app = %ctx.app.name, "redis: app destroyed, service links removed by cascade");
        Ok(())
    }
}

#[no_mangle]
pub extern "C" fn deku_plugin_create() -> *mut dyn PluginDescriptor {
    Box::into_raw(Box::new(RedisPlugin))
}
