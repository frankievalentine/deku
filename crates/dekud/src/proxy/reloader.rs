use anyhow::Result;
use std::path::Path;

fn env_override(name: &str) -> Option<String> {
    if deku_core::dev_hooks::enabled() {
        std::env::var(name).ok()
    } else {
        None
    }
}

fn angie_bin() -> String {
    env_override("DEKU_ANGIE_BIN").unwrap_or_else(|| "angie".to_string())
}

fn kill_bin() -> String {
    env_override("DEKU_KILL_BIN").unwrap_or_else(|| "kill".to_string())
}

fn pid_path() -> std::path::PathBuf {
    env_override("DEKU_ANGIE_PID_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/run/angie/angie.pid"))
}

pub async fn validate() -> Result<()> {
    let output = tokio::process::Command::new(angie_bin())
        .args(["-t", "-q"])
        .output()
        .await?;

    if output.status.success() {
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::bail!("angie config validation failed: {}{}", stdout, stderr);
}

/// The effective configuration, as Angie resolves it.
///
/// `angie -T` writes every included file, each preceded by a marker naming its
/// path, which is how a caller can tell whether a directory is included at all.
/// An include that is missing is otherwise invisible: the files sit on disk and
/// Angie never reads them.
pub async fn dump() -> Result<String> {
    let output = tokio::process::Command::new(angie_bin())
        .args(["-T"])
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!(
            "angie -T failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// The configuration files Angie actually loaded, from `-T` output.
pub fn loaded_config_files(dump: &str) -> Vec<String> {
    dump.lines()
        .filter_map(|line| line.trim().strip_prefix("# configuration file "))
        .filter_map(|rest| rest.strip_suffix(':'))
        .map(|path| path.to_string())
        .collect()
}

/// Reload Angie configuration.
///
/// Sends SIGHUP to the running angie process found in the standard pid file.
/// If the pid file doesn't exist (e.g. Angie not installed yet), this is a
/// no-op with a debug log rather than an error — callers should not fail
/// deploys because Angie isn't installed yet.
pub async fn reload() -> Result<()> {
    let pid_path = pid_path();

    if !pid_path.exists() {
        tracing::debug!("angie pid file not found — skipping reload");
        return Ok(());
    }

    validate().await?;

    let pid_str = tokio::fs::read_to_string(&pid_path).await?;
    let pid = pid_str
        .trim()
        .parse::<u32>()
        .map_err(|_| anyhow::anyhow!("invalid angie pid: {pid_str}"))?;

    let status = tokio::process::Command::new(kill_bin())
        .args(["-HUP", &pid.to_string()])
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("failed to send SIGHUP to angie (pid {pid})");
    }

    tracing::info!(pid, "angie reloaded");
    Ok(())
}

/// The app config file names in a directory, which Angie is expected to load.
///
/// Compared by name rather than by full path: an include may reach the
/// directory through a symlink or a different spelling, and the question is
/// whether Angie reads the file at all, not how it spells the path.
pub fn app_config_files(conf_dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(conf_dir) else {
        return Vec::new();
    };

    let mut files: Vec<String> = entries
        .flatten()
        // A directory can be named `something.conf`; only files are configs.
        .filter(|entry| entry.path().is_file())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "conf"))
        .filter_map(|entry| {
            entry
                .path()
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .collect();
    files.sort();
    files
}

/// The app configs on disk that the effective configuration does not load.
pub fn missing_app_configs(expected: &[String], loaded: &[String]) -> Vec<String> {
    let loaded_names: Vec<&str> = loaded
        .iter()
        .filter_map(|path| path.rsplit('/').next())
        .collect();

    expected
        .iter()
        .filter(|name| !loaded_names.contains(&name.as_str()))
        .cloned()
        .collect()
}

/// What to do about the app config include.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncludeAction {
    /// No app configs yet, so there is nothing to decide.
    NothingToLoad,
    /// Angie already loads them.
    AlreadyLoaded,
    /// Write the drop-in that includes the directory.
    WriteDropIn,
    /// A drop-in is present and the configs are still not loaded, so the
    /// missing include is not the explanation. Leave it alone rather than
    /// rewriting it on every start.
    Unexplained,
}

/// Decide whether the app config directory needs a drop-in include.
pub fn include_action(
    expected: &[String],
    loaded: &[String],
    drop_in_exists: bool,
) -> IncludeAction {
    if expected.is_empty() {
        return IncludeAction::NothingToLoad;
    }
    if missing_app_configs(expected, loaded).is_empty() {
        return IncludeAction::AlreadyLoaded;
    }
    if drop_in_exists {
        return IncludeAction::Unexplained;
    }
    IncludeAction::WriteDropIn
}

/// The drop-in that makes Angie load the app config directory.
///
/// It sits beside the directory rather than in it, so it is matched by the
/// distribution's own `conf.d` include, and its name uses a character app
/// configs cannot (app names allow only letters, digits, `-`, `_`, and `.`), so
/// it can never be confused with an app called `deku-apps`.
pub fn app_include_path(conf_dir: &Path) -> Option<std::path::PathBuf> {
    Some(conf_dir.parent()?.join("deku-apps.conf"))
}

/// The drop-in's contents: one include of the app config directory.
pub fn app_include_contents(conf_dir: &Path) -> String {
    format!(
        "# Written by Deku so Angie loads the app vhosts. Do not edit.\ninclude {}/*.conf;\n",
        conf_dir.display()
    )
}

/// Ensure Angie loads the app config directory, returning what was done.
///
/// Whether the distribution's own configuration includes this subdirectory is
/// not something Deku controls, so it is checked rather than assumed. Writing
/// the drop-in when the directory is already included would include it twice,
/// and Angie rejects the duplicate upstreams that would produce.
pub async fn ensure_app_config_include(conf_dir: &Path) -> Result<IncludeAction> {
    let expected = app_config_files(conf_dir);
    let drop_in = app_include_path(conf_dir);
    let drop_in_exists = drop_in.as_ref().is_some_and(|path| path.exists());

    // With no configs to load there is nothing to conclude: an empty directory
    // is indistinguishable from an unloaded one. The first config written will
    // bring us back here.
    if expected.is_empty() {
        return Ok(IncludeAction::NothingToLoad);
    }

    let loaded = loaded_config_files(&dump().await?);
    let action = include_action(&expected, &loaded, drop_in_exists);

    if action == IncludeAction::WriteDropIn {
        let path = drop_in.ok_or_else(|| {
            anyhow::anyhow!(
                "cannot place an include beside {}: it has no parent directory",
                conf_dir.display()
            )
        })?;
        std::fs::write(&path, app_include_contents(conf_dir))
            .with_context_label(&path, "writing the Angie app config include")?;
        tracing::info!(path = %path.display(), "added the Angie include for app configs");
    }

    Ok(action)
}

/// A small context helper so a write failure names the file.
trait WithContextLabel<T> {
    fn with_context_label(self, path: &Path, what: &str) -> Result<T>;
}

impl<T> WithContextLabel<T> for std::io::Result<T> {
    fn with_context_label(self, path: &Path, what: &str) -> Result<T> {
        self.map_err(|error| anyhow::anyhow!("{what} at {}: {error}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        app_config_files, app_include_contents, app_include_path, include_action,
        missing_app_configs, IncludeAction,
    };

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn only_config_files_are_expected() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("demo.conf"), "server {}").expect("write");
        std::fs::write(dir.path().join("notes.txt"), "ignore me").expect("write");
        std::fs::create_dir(dir.path().join("sub.conf")).expect("dir");

        assert_eq!(app_config_files(dir.path()), names(&["demo.conf"]));
    }

    #[test]
    fn a_config_the_effective_configuration_omits_is_reported() {
        // Angie's `-T` output names every file it loaded.
        let loaded = names(&["/etc/angie/angie.conf", "/etc/angie/conf.d/deku/demo.conf"]);
        assert_eq!(
            missing_app_configs(&names(&["demo.conf", "shop.conf"]), &loaded),
            names(&["shop.conf"]),
            "a config that is not loaded must be named"
        );
    }

    #[test]
    fn nothing_is_reported_when_every_config_is_loaded() {
        let loaded = names(&["/etc/angie/conf.d/deku/demo.conf"]);
        assert!(missing_app_configs(&names(&["demo.conf"]), &loaded).is_empty());
    }

    #[test]
    fn the_include_is_written_only_when_it_is_missing() {
        let expected = names(&["demo.conf"]);
        let not_loaded = names(&["/etc/angie/angie.conf"]);

        assert_eq!(
            include_action(&expected, &not_loaded, false),
            IncludeAction::WriteDropIn
        );
        // Already included: writing again would include the directory twice and
        // Angie would reject the duplicate upstreams.
        assert_eq!(
            include_action(&expected, &loaded_with("demo.conf"), false),
            IncludeAction::AlreadyLoaded
        );
        // Nothing on disk to load: an empty directory looks the same whether it
        // is included or not, so there is nothing to decide.
        assert_eq!(
            include_action(&[], &not_loaded, false),
            IncludeAction::NothingToLoad
        );
        // A drop-in is present and it still is not loaded, so the include is not
        // the explanation. Rewriting it every start would hide that.
        assert_eq!(
            include_action(&expected, &not_loaded, true),
            IncludeAction::Unexplained
        );
    }

    fn loaded_with(name: &str) -> Vec<String> {
        names(&[&format!("/etc/angie/conf.d/deku/{name}")])
    }

    #[test]
    fn the_include_sits_beside_the_directory() {
        let conf_dir = std::path::Path::new("/etc/angie/conf.d/deku");
        assert_eq!(
            app_include_path(conf_dir),
            Some(std::path::PathBuf::from("/etc/angie/conf.d/deku-apps.conf"))
        );
    }

    #[test]
    fn the_include_names_the_app_config_directory() {
        let conf_dir = std::path::Path::new("/etc/angie/conf.d/deku");
        assert_eq!(
            app_include_contents(conf_dir),
            "# Written by Deku so Angie loads the app vhosts. Do not edit.\ninclude /etc/angie/conf.d/deku/*.conf;\n"
        );
    }
}
