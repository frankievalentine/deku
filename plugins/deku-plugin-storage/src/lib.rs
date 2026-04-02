#![allow(unsafe_code)]

use async_trait::async_trait;
use deku_plugin_sdk::error::Result;
use deku_plugin_sdk::{context::DeployContext, hooks::PreDeployHook, PluginDescriptor};

pub struct StoragePlugin;

impl PluginDescriptor for StoragePlugin {
    fn name(&self) -> &'static str {
        "storage"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn pre_deploy(&self) -> Option<Box<dyn PreDeployHook>> {
        Some(Box::new(StoragePreDeployHook))
    }
}

struct StoragePreDeployHook;

#[async_trait]
impl PreDeployHook for StoragePreDeployHook {
    async fn pre_deploy(&self, ctx: &DeployContext) -> Result<()> {
        tracing::info!(app = %ctx.app.name, "storage: pre_deploy for app {}", ctx.app.name);
        Ok(())
    }
}

#[no_mangle]
pub extern "C" fn deku_plugin_create() -> *mut dyn PluginDescriptor {
    Box::into_raw(Box::new(StoragePlugin))
}
