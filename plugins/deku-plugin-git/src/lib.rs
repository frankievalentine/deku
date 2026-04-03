#![allow(unsafe_code)]

use async_trait::async_trait;
use deku_plugin_sdk::error::Result;
use deku_plugin_sdk::{
    context::AppContext,
    hooks::{AppCreateHook, AppDestroyHook},
    PluginDescriptor,
};

pub struct GitPlugin;

impl PluginDescriptor for GitPlugin {
    fn name(&self) -> &'static str {
        "git"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn app_create(&self) -> Option<Box<dyn AppCreateHook>> {
        Some(Box::new(GitCreateHook))
    }

    fn app_destroy(&self) -> Option<Box<dyn AppDestroyHook>> {
        Some(Box::new(GitDestroyHook))
    }
}

struct GitCreateHook;

#[async_trait]
impl AppCreateHook for GitCreateHook {
    async fn app_create(&self, ctx: &AppContext) -> Result<()> {
        let repo_path = ctx
            .data_dir
            .join("git-repos")
            .join(format!("{}.git", ctx.app.name));
        let repo_path_str = repo_path.to_string_lossy().to_string();
        std::fs::create_dir_all(&repo_path)?;
        std::process::Command::new("git")
            .args(["init", "--bare", &repo_path_str])
            .output()?;
        tracing::info!(app = %ctx.app.name, "git: initialized bare repo at {repo_path_str}");
        Ok(())
    }
}

struct GitDestroyHook;

#[async_trait]
impl AppDestroyHook for GitDestroyHook {
    async fn app_destroy(&self, ctx: &AppContext) -> Result<()> {
        let repo_path = ctx
            .data_dir
            .join("git-repos")
            .join(format!("{}.git", ctx.app.name));
        let _ = std::fs::remove_dir_all(&repo_path);
        tracing::info!(app = %ctx.app.name, "git: removed bare repo for app {}", ctx.app.name);
        Ok(())
    }
}

#[no_mangle]
pub extern "C" fn deku_plugin_create() -> *mut dyn PluginDescriptor {
    Box::into_raw(Box::new(GitPlugin))
}
