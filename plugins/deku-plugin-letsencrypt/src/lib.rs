#![allow(unsafe_code)]

use async_trait::async_trait;
use deku_plugin_sdk::error::Result;
use deku_plugin_sdk::{context::DeployContext, hooks::PostDeployHook, PluginDescriptor};

pub struct LetsencryptPlugin;

impl PluginDescriptor for LetsencryptPlugin {
    fn name(&self) -> &'static str {
        "letsencrypt"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn post_deploy(&self) -> Option<Box<dyn PostDeployHook>> {
        Some(Box::new(LePostDeployHook))
    }
}

struct LePostDeployHook;

#[async_trait]
impl PostDeployHook for LePostDeployHook {
    async fn post_deploy(&self, ctx: &DeployContext) -> Result<()> {
        tracing::info!(app = %ctx.app.name, "letsencrypt: post_deploy for app {}", ctx.app.name);
        Ok(())
    }
}

#[no_mangle]
pub extern "C" fn deku_plugin_create() -> *mut dyn PluginDescriptor {
    Box::into_raw(Box::new(LetsencryptPlugin))
}
