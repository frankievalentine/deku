#![allow(unsafe_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use libloading::{Library, Symbol};
use tokio::sync::RwLock;

use deku_plugin_sdk::context::{AppContext, BuildContext, DeployContext};
pub use deku_plugin_sdk::PluginDescriptor;

/// Symbol exported by each plugin shared library.
type PluginCreateFn = unsafe fn() -> *mut dyn PluginDescriptor;

struct LoadedPlugin {
    descriptor: Box<dyn PluginDescriptor>,
    /// Keep the library alive for the lifetime of the plugin.
    _lib: Library,
    path: PathBuf,
}

#[derive(Default)]
pub struct PluginRegistry {
    plugins: RwLock<HashMap<String, LoadedPlugin>>,
}

impl PluginRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            plugins: RwLock::new(HashMap::new()),
        })
    }

    /// Load all `.so` / `.dylib` files from `plugins_dir`.
    pub async fn load_all(&self, plugins_dir: &Path) -> Result<()> {
        if !plugins_dir.exists() {
            return Ok(());
        }
        let entries = std::fs::read_dir(plugins_dir)?;
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext == "so" || ext == "dylib" {
                if let Err(e) = self.load_plugin(&path).await {
                    tracing::warn!(path = %path.display(), "failed to load plugin: {e}");
                }
            }
        }
        Ok(())
    }

    /// Load a single plugin `.so`/`.dylib` by path.
    pub async fn load_plugin(&self, path: &Path) -> Result<String> {
        let lib = unsafe { Library::new(path) }
            .map_err(|e| anyhow!("failed to open {}: {e}", path.display()))?;

        let descriptor: Box<dyn PluginDescriptor> = unsafe {
            let create: Symbol<PluginCreateFn> = lib
                .get(b"deku_plugin_create\0")
                .map_err(|e| anyhow!("missing deku_plugin_create symbol: {e}"))?;
            Box::from_raw(create())
        };

        let name = descriptor.name().to_string();
        tracing::info!(plugin = %name, version = descriptor.version(), "plugin loaded");

        let loaded = LoadedPlugin {
            descriptor,
            _lib: lib,
            path: path.to_path_buf(),
        };

        self.plugins.write().await.insert(name.clone(), loaded);
        Ok(name)
    }

    /// Unload a plugin by name.
    pub async fn unload_plugin(&self, name: &str) -> Result<()> {
        let removed = self.plugins.write().await.remove(name);
        if removed.is_none() {
            return Err(anyhow!("plugin '{name}' not found"));
        }
        tracing::info!(plugin = %name, "plugin unloaded");
        Ok(())
    }

    /// List all loaded plugins as `(name, version, path)` tuples.
    pub async fn list_plugins(&self) -> Vec<(String, String, PathBuf)> {
        self.plugins
            .read()
            .await
            .values()
            .map(|p| {
                (
                    p.descriptor.name().to_string(),
                    p.descriptor.version().to_string(),
                    p.path.clone(),
                )
            })
            .collect()
    }

    // ── Hook dispatchers ──────────────────────────────────────────────────────

    pub async fn run_pre_build(&self, ctx: &BuildContext) {
        let guard = self.plugins.read().await;
        for plugin in guard.values() {
            if let Some(hook) = plugin.descriptor.pre_build() {
                if let Err(e) = hook.pre_build(ctx).await {
                    tracing::warn!(
                        plugin = plugin.descriptor.name(),
                        "pre_build hook error: {e}"
                    );
                }
            }
        }
    }

    pub async fn run_post_build(&self, ctx: &BuildContext) {
        let guard = self.plugins.read().await;
        for plugin in guard.values() {
            if let Some(hook) = plugin.descriptor.post_build() {
                if let Err(e) = hook.post_build(ctx).await {
                    tracing::warn!(
                        plugin = plugin.descriptor.name(),
                        "post_build hook error: {e}"
                    );
                }
            }
        }
    }

    pub async fn run_pre_deploy(&self, ctx: &DeployContext) {
        let guard = self.plugins.read().await;
        for plugin in guard.values() {
            if let Some(hook) = plugin.descriptor.pre_deploy() {
                if let Err(e) = hook.pre_deploy(ctx).await {
                    tracing::warn!(
                        plugin = plugin.descriptor.name(),
                        "pre_deploy hook error: {e}"
                    );
                }
            }
        }
    }

    pub async fn run_post_deploy(&self, ctx: &DeployContext) {
        let guard = self.plugins.read().await;
        for plugin in guard.values() {
            if let Some(hook) = plugin.descriptor.post_deploy() {
                if let Err(e) = hook.post_deploy(ctx).await {
                    tracing::warn!(
                        plugin = plugin.descriptor.name(),
                        "post_deploy hook error: {e}"
                    );
                }
            }
        }
    }

    pub async fn run_app_create(&self, ctx: &AppContext) {
        let guard = self.plugins.read().await;
        for plugin in guard.values() {
            if let Some(hook) = plugin.descriptor.app_create() {
                if let Err(e) = hook.app_create(ctx).await {
                    tracing::warn!(
                        plugin = plugin.descriptor.name(),
                        "app_create hook error: {e}"
                    );
                }
            }
        }
    }

    pub async fn run_app_destroy(&self, ctx: &AppContext) {
        let guard = self.plugins.read().await;
        for plugin in guard.values() {
            if let Some(hook) = plugin.descriptor.app_destroy() {
                if let Err(e) = hook.app_destroy(ctx).await {
                    tracing::warn!(
                        plugin = plugin.descriptor.name(),
                        "app_destroy hook error: {e}"
                    );
                }
            }
        }
    }
}
