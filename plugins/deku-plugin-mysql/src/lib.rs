#![allow(unsafe_code)]

use async_trait::async_trait;
use deku_plugin_sdk::error::Result;
use deku_plugin_sdk::{context::AppContext, hooks::AppDestroyHook, PluginDescriptor};

pub struct MysqlPlugin;

impl PluginDescriptor for MysqlPlugin {
    fn name(&self) -> &'static str {
        "mysql"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn app_destroy(&self) -> Option<Box<dyn AppDestroyHook>> {
        Some(Box::new(MyDestroyHook))
    }
}

struct MyDestroyHook;

#[async_trait]
impl AppDestroyHook for MyDestroyHook {
    async fn app_destroy(&self, ctx: &AppContext) -> Result<()> {
        tracing::info!(app = %ctx.app.name, "mysql: app destroyed, service links removed by cascade");
        Ok(())
    }
}

#[no_mangle]
pub extern "C" fn deku_plugin_create() -> *mut dyn PluginDescriptor {
    Box::into_raw(Box::new(MysqlPlugin))
}
