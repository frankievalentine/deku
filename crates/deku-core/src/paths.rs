//! Where Deku keeps its config and state.
//!
//! The CLI and the daemon both resolve the config directory here, so they always
//! agree on which `config.toml` is in use.
//!
//! Installs from before the system layout keep everything under `~/.deku` and
//! continue to use it. New installs on Linux, which run as root, use
//! `/etc/deku` for config and `/var/lib/deku` for state.

use std::path::{Path, PathBuf};

/// Config directory of a system install.
pub const SYSTEM_CONFIG_DIR: &str = "/etc/deku";

/// State directory of a system install: database, logs, dashboard, socket.
pub const SYSTEM_DATA_DIR: &str = "/var/lib/deku";

/// The per-user directory installs from before the system layout use.
pub fn legacy_config_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/root"))
        .join(".deku")
}

/// The config directory: `DEKU_CONFIG_DIR` when set, otherwise whichever of the
/// system and legacy directories already holds a config, otherwise the layout a
/// new install on this host gets.
pub fn config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("DEKU_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    resolve_config_dir(
        Path::new(SYSTEM_CONFIG_DIR),
        &legacy_config_dir(),
        uses_system_layout(),
    )
}

/// The default state directory for a config directory: a system install keeps
/// state apart from config, a legacy install keeps both together.
pub fn default_data_dir_for(config_dir: &Path) -> PathBuf {
    if config_dir == Path::new(SYSTEM_CONFIG_DIR) {
        PathBuf::from(SYSTEM_DATA_DIR)
    } else {
        config_dir.to_path_buf()
    }
}

fn resolve_config_dir(system: &Path, legacy: &Path, system_layout: bool) -> PathBuf {
    if system.join("config.toml").exists() {
        system.to_path_buf()
    } else if legacy.join("config.toml").exists() || !system_layout {
        legacy.to_path_buf()
    } else {
        system.to_path_buf()
    }
}

/// Whether a new install on this host uses the system layout: Linux, as root.
/// Anything else, such as a contributor's source checkout, keeps `~/.deku`.
fn uses_system_layout() -> bool {
    #[cfg(target_os = "linux")]
    {
        rustix::process::geteuid().is_root()
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::{default_data_dir_for, resolve_config_dir, SYSTEM_CONFIG_DIR, SYSTEM_DATA_DIR};
    use std::path::{Path, PathBuf};

    fn dirs() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let root = tempfile::tempdir().expect("tempdir");
        let system = root.path().join("etc-deku");
        let legacy = root.path().join("home-deku");
        std::fs::create_dir_all(&system).expect("system dir");
        std::fs::create_dir_all(&legacy).expect("legacy dir");
        (root, system, legacy)
    }

    #[test]
    fn an_existing_legacy_install_keeps_its_directory() {
        let (_root, system, legacy) = dirs();
        std::fs::write(legacy.join("config.toml"), "").expect("legacy config");
        assert_eq!(resolve_config_dir(&system, &legacy, true), legacy);
    }

    #[test]
    fn a_system_install_wins_when_both_exist() {
        let (_root, system, legacy) = dirs();
        std::fs::write(system.join("config.toml"), "").expect("system config");
        std::fs::write(legacy.join("config.toml"), "").expect("legacy config");
        assert_eq!(resolve_config_dir(&system, &legacy, true), system);
    }

    #[test]
    fn a_new_install_follows_the_host_layout() {
        let (_root, system, legacy) = dirs();
        assert_eq!(resolve_config_dir(&system, &legacy, true), system);
        assert_eq!(resolve_config_dir(&system, &legacy, false), legacy);
    }

    #[test]
    fn system_installs_keep_state_apart_from_config() {
        assert_eq!(
            default_data_dir_for(Path::new(SYSTEM_CONFIG_DIR)),
            PathBuf::from(SYSTEM_DATA_DIR)
        );
        let legacy = Path::new("/root/.deku");
        assert_eq!(default_data_dir_for(legacy), legacy.to_path_buf());
    }
}
