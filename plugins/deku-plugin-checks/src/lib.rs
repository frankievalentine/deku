#![allow(unsafe_code)]

use async_trait::async_trait;
use deku_plugin_sdk::error::Result;
use deku_plugin_sdk::{context::DeployContext, hooks::PostDeployHook, PluginDescriptor};

pub struct ChecksPlugin;

impl PluginDescriptor for ChecksPlugin {
    fn name(&self) -> &'static str {
        "checks"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn post_deploy(&self) -> Option<Box<dyn PostDeployHook>> {
        Some(Box::new(ChecksPostDeployHook))
    }
}

struct ChecksPostDeployHook;

#[async_trait]
impl PostDeployHook for ChecksPostDeployHook {
    async fn post_deploy(&self, ctx: &DeployContext) -> Result<()> {
        tracing::info!(
            app = %ctx.app.name,
            deployment = %ctx.deployment.id,
            "checks: deployment {} for app {} completed",
            ctx.deployment.id,
            ctx.app.name
        );
        Ok(())
    }
}

#[no_mangle]
pub extern "C" fn deku_plugin_create() -> *mut dyn PluginDescriptor {
    Box::into_raw(Box::new(ChecksPlugin))
}
