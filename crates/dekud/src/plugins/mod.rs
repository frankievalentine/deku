//! In-process cdylib plugin runtime, compiled only with the `dynamic-plugins`
//! feature.
//!
//! Loading a foreign shared library into the daemon is unsafe by construction:
//! a plugin built against a different `rustc` or `std` can abort the process.
//! The runtime is therefore off by default, and out-of-process hooks
//! ([`crate::hooks`]) are the supported integration point.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;

use deku_plugin_sdk::context::{AppContext, BuildContext, DeployContext};
// Only the loader path names the trait directly.
#[cfg_attr(not(feature = "dynamic-plugins"), allow(unused_imports))]
pub use deku_plugin_sdk::PluginDescriptor;

#[cfg(feature = "dynamic-plugins")]
mod loader;

/// Whether this binary was built with the in-process plugin runtime.
pub const fn dynamic_runtime_available() -> bool {
    cfg!(feature = "dynamic-plugins")
}

/// Error used when a plugin operation needs a runtime this build lacks.
pub fn runtime_unavailable_message() -> &'static str {
    "this dekud build does not include the dynamic plugin runtime; \
     rebuild with --features dynamic-plugins, or use out-of-process hooks"
}

/// Dispatches one hook kind to every loaded plugin.
///
/// The body compiles to nothing when the runtime is disabled, which keeps the
/// six dispatchers below to a single line each.
macro_rules! dispatch_hooks {
    ($registry:expr, $ctx:expr, $accessor:ident) => {{
        #[cfg(feature = "dynamic-plugins")]
        {
            let guard = $registry.plugins.read().await;
            for plugin in guard.values() {
                if let Some(hook) = plugin.descriptor.$accessor() {
                    if let Err(e) = hook.$accessor($ctx).await {
                        tracing::warn!(
                            plugin = plugin.descriptor.name(),
                            "{} hook error: {e}",
                            stringify!($accessor)
                        );
                    }
                }
            }
        }
        #[cfg(not(feature = "dynamic-plugins"))]
        {
            let _ = $ctx;
        }
    }};
}

pub struct PluginRegistry {
    #[cfg(feature = "dynamic-plugins")]
    plugins: loader::LoadedMap,
}

impl PluginRegistry {
    pub fn new() -> Arc<Self> {
        #[cfg(feature = "dynamic-plugins")]
        {
            Arc::new(Self {
                plugins: loader::empty_map(),
            })
        }
        #[cfg(not(feature = "dynamic-plugins"))]
        {
            Arc::new(Self {})
        }
    }

    /// Load all `.so` / `.dylib` files from `plugins_dir`.
    pub async fn load_all(&self, plugins_dir: &Path) -> Result<()> {
        #[cfg(feature = "dynamic-plugins")]
        {
            if !plugins_dir.exists() {
                return Ok(());
            }
            for entry in std::fs::read_dir(plugins_dir)? {
                let path = entry?.path();
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext == "so" || ext == "dylib" {
                    if let Err(e) = self.load_plugin(&path).await {
                        tracing::warn!(path = %path.display(), "failed to load plugin: {e}");
                    }
                }
            }
            Ok(())
        }

        // Without the runtime, say so when libraries are sitting in the directory
        // rather than silently ignoring them.
        #[cfg(not(feature = "dynamic-plugins"))]
        {
            let libraries = std::fs::read_dir(plugins_dir)
                .map(|entries| {
                    entries
                        .flatten()
                        .filter(|entry| {
                            let path = entry.path();
                            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                            ext == "so" || ext == "dylib"
                        })
                        .count()
                })
                .unwrap_or(0);
            if libraries > 0 {
                tracing::warn!(
                    dir = %plugins_dir.display(),
                    libraries,
                    "{}",
                    runtime_unavailable_message()
                );
            }
            Ok(())
        }
    }

    /// Load a single plugin `.so`/`.dylib` by path.
    pub async fn load_plugin(&self, path: &Path) -> Result<String> {
        #[cfg(feature = "dynamic-plugins")]
        {
            let loaded = loader::open(path)?;
            let name = loaded.descriptor.name().to_string();
            self.plugins.write().await.insert(name.clone(), loaded);
            Ok(name)
        }
        #[cfg(not(feature = "dynamic-plugins"))]
        {
            let _ = path;
            anyhow::bail!("{}", runtime_unavailable_message())
        }
    }

    /// Unload a plugin by name.
    pub async fn unload_plugin(&self, name: &str) -> Result<()> {
        #[cfg(feature = "dynamic-plugins")]
        {
            let removed = self.plugins.write().await.remove(name);
            if removed.is_none() {
                anyhow::bail!("plugin '{name}' not found");
            }
            tracing::info!(plugin = %name, "plugin unloaded");
            Ok(())
        }
        #[cfg(not(feature = "dynamic-plugins"))]
        {
            let _ = name;
            anyhow::bail!("{}", runtime_unavailable_message())
        }
    }

    /// List all loaded plugins as `(name, version, path)` tuples.
    pub async fn list_plugins(&self) -> Vec<(String, String, PathBuf)> {
        #[cfg(feature = "dynamic-plugins")]
        {
            self.plugins
                .read()
                .await
                .values()
                .map(|plugin| {
                    (
                        plugin.descriptor.name().to_string(),
                        plugin.descriptor.version().to_string(),
                        plugin.path.clone(),
                    )
                })
                .collect()
        }
        #[cfg(not(feature = "dynamic-plugins"))]
        {
            Vec::new()
        }
    }

    // ── Hook dispatchers ──────────────────────────────────────────────────────

    pub async fn run_pre_build(&self, ctx: &BuildContext) {
        dispatch_hooks!(self, ctx, pre_build);
    }

    pub async fn run_post_build(&self, ctx: &BuildContext) {
        dispatch_hooks!(self, ctx, post_build);
    }

    pub async fn run_pre_deploy(&self, ctx: &DeployContext) {
        dispatch_hooks!(self, ctx, pre_deploy);
    }

    pub async fn run_post_deploy(&self, ctx: &DeployContext) {
        dispatch_hooks!(self, ctx, post_deploy);
    }

    pub async fn run_app_create(&self, ctx: &AppContext) {
        dispatch_hooks!(self, ctx, app_create);
    }

    pub async fn run_app_destroy(&self, ctx: &AppContext) {
        dispatch_hooks!(self, ctx, app_destroy);
    }
}
