//! Shared-library loading for the `dynamic-plugins` feature.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use libloading::{Library, Symbol};
use tokio::sync::RwLock;

use super::PluginDescriptor;

/// Symbol exported by each plugin shared library.
type PluginCreateFn = unsafe fn() -> *mut dyn PluginDescriptor;

pub(super) struct LoadedPlugin {
    pub(super) descriptor: Box<dyn PluginDescriptor>,
    /// Keeps the library mapped for the lifetime of the plugin.
    pub(super) _lib: Library,
    pub(super) path: PathBuf,
}

pub(super) type LoadedMap = RwLock<HashMap<String, LoadedPlugin>>;

pub(super) fn empty_map() -> LoadedMap {
    RwLock::new(HashMap::new())
}

/// Open a shared library and build its descriptor.
pub(super) fn open(path: &Path) -> Result<LoadedPlugin> {
    let lib = unsafe { Library::new(path) }
        .map_err(|e| anyhow!("failed to open {}: {e}", path.display()))?;

    let descriptor: Box<dyn PluginDescriptor> = unsafe {
        let create: Symbol<PluginCreateFn> = lib
            .get(b"deku_plugin_create\0")
            .map_err(|e| anyhow!("missing deku_plugin_create symbol: {e}"))?;
        Box::from_raw(create())
    };

    tracing::info!(
        plugin = descriptor.name(),
        version = descriptor.version(),
        "plugin loaded"
    );

    Ok(LoadedPlugin {
        descriptor,
        _lib: lib,
        path: path.to_path_buf(),
    })
}
