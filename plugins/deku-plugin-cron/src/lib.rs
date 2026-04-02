#![allow(unsafe_code)]

use async_trait::async_trait;
use deku_plugin_sdk::error::Result;
use deku_plugin_sdk::{
    context::{AppContext, DeployContext},
    hooks::{AppDestroyHook, PostDeployHook},
    PluginDescriptor,
};

pub struct CronPlugin;

impl PluginDescriptor for CronPlugin {
    fn name(&self) -> &'static str {
        "cron"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn post_deploy(&self) -> Option<Box<dyn PostDeployHook>> {
        Some(Box::new(CronPostDeployHook))
    }

    fn app_destroy(&self) -> Option<Box<dyn AppDestroyHook>> {
        Some(Box::new(CronDestroyHook))
    }
}

struct CronPostDeployHook;

#[async_trait]
impl PostDeployHook for CronPostDeployHook {
    async fn post_deploy(&self, ctx: &DeployContext) -> Result<()> {
        tracing::info!(app = %ctx.app.name, "cron: post_deploy for app {}", ctx.app.name);
        Ok(())
    }
}

struct CronDestroyHook;

#[async_trait]
impl AppDestroyHook for CronDestroyHook {
    async fn app_destroy(&self, ctx: &AppContext) -> Result<()> {
        tracing::info!(app = %ctx.app.name, "cron: app_destroy for app {}", ctx.app.name);
        Ok(())
    }
}

#[no_mangle]
pub extern "C" fn deku_plugin_create() -> *mut dyn PluginDescriptor {
    Box::into_raw(Box::new(CronPlugin))
}
