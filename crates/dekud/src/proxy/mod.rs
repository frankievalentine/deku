pub mod reloader;
pub mod writer;

use anyhow::{Context, Result};
use deku_core::types::Upstream;
use std::path::{Path, PathBuf};

pub use reloader::reload;
pub use writer::{
    app_config_path, read_app_config, remove_app_config, write_app_config, write_raw_app_config,
};

pub struct DesiredAppConfig<'a> {
    pub domains: &'a [String],
    pub upstreams: &'a [Upstream],
    pub tls: bool,
    /// Per-app auth from `app_auth`, or `None` when the app is public.
    pub auth: Option<&'a crate::db::queries::AppAuthRecord>,
    /// When true, the app serves a 503 instead of proxying.
    pub maintenance: bool,
    pub maintenance_message: Option<&'a str>,
    pub redirects: &'a [crate::db::queries::Redirect],
}

/// Per-app proxy state loaded from the database.
#[derive(Default)]
pub struct AppProxyExtras {
    pub auth: Option<crate::db::queries::AppAuthRecord>,
    pub maintenance: bool,
    pub maintenance_message: Option<String>,
    pub redirects: Vec<crate::db::queries::Redirect>,
}

/// Load auth, maintenance, and redirects for an app.
pub async fn load_extras(pool: &sqlx::SqlitePool, app_id: &str) -> Result<AppProxyExtras> {
    let auth = crate::db::queries::get_app_auth(pool, app_id).await?;
    let (maintenance, maintenance_message) =
        crate::db::queries::get_app_maintenance(pool, app_id).await?;
    let redirects = crate::db::queries::list_redirects(pool, app_id).await?;
    Ok(AppProxyExtras {
        auth,
        maintenance,
        maintenance_message,
        redirects,
    })
}

pub fn cert_path(app_name: &str) -> PathBuf {
    PathBuf::from(format!("/etc/angie/ssl/deku_{app_name}.crt"))
}

pub fn key_path(app_name: &str) -> PathBuf {
    PathBuf::from(format!("/etc/angie/ssl/deku_{app_name}.key"))
}

pub async fn apply_app_config(
    conf_dir: &Path,
    app_name: &str,
    desired: Option<DesiredAppConfig<'_>>,
) -> Result<()> {
    let previous = read_app_config(conf_dir, app_name)?;

    match desired {
        Some(config) => write_app_config(
            conf_dir,
            writer::VhostConfig {
                app_name,
                domains: config.domains,
                upstreams: config.upstreams,
                tls: config.tls,
                auth: config.auth,
                maintenance: config.maintenance,
                maintenance_message: config.maintenance_message,
                redirects: config.redirects,
            },
        )?,
        None => remove_app_config(conf_dir, app_name)?,
    }

    if let Err(error) = reload().await {
        tracing::warn!(
            app = app_name,
            "angie apply failed, restoring previous config: {error}"
        );
        restore_previous_app_config(conf_dir, app_name, previous.as_deref())
            .with_context(|| format!("restoring previous angie config for {app_name}"))?;

        if let Err(rollback_error) = reload().await {
            return Err(error).context(format!(
                "failed to apply angie config for {app_name}; rollback reload also failed: {rollback_error}"
            ));
        }

        return Err(error).context(format!(
            "failed to apply angie config for {app_name}; restored previous config"
        ));
    }

    Ok(())
}

fn restore_previous_app_config(
    conf_dir: &Path,
    app_name: &str,
    previous: Option<&[u8]>,
) -> Result<()> {
    match previous {
        Some(contents) => write_raw_app_config(conf_dir, app_name, contents),
        None => remove_app_config(conf_dir, app_name),
    }
}

#[cfg(test)]
mod tests {
    use super::{app_config_path, apply_app_config, write_raw_app_config, DesiredAppConfig};
    use deku_core::types::Upstream;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::OnceLock;
    use tokio::sync::Mutex;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn write_executable(path: &std::path::Path, body: &str) {
        std::fs::write(path, body).expect("script should write");
        let mut perms = std::fs::metadata(path)
            .expect("metadata should load")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).expect("permissions should set");
    }

    #[tokio::test]
    async fn restores_previous_config_when_reload_fails() {
        let _guard = env_lock().lock().await;
        let temp = tempfile::tempdir().expect("tempdir should create");
        let conf_dir = temp.path().join("conf");
        std::fs::create_dir_all(&conf_dir).expect("conf dir should create");

        let bin_dir = temp.path().join("bin");
        std::fs::create_dir_all(&bin_dir).expect("bin dir should create");
        let angie = bin_dir.join("angie");
        write_executable(&angie, "#!/bin/sh\nexit 1\n");

        let pid_path = temp.path().join("angie.pid");
        std::fs::write(&pid_path, "123\n").expect("pid file should write");

        std::env::set_var("DEKU_ANGIE_BIN", &angie);
        std::env::set_var("DEKU_ANGIE_PID_PATH", &pid_path);

        let previous = b"previous config\n";
        write_raw_app_config(&conf_dir, "demo", previous).expect("previous config should write");

        let upstreams = vec![Upstream {
            host: "127.0.0.1".to_string(),
            port: 8080,
        }];
        let domains = vec![String::from("example.com")];
        let result = apply_app_config(
            &conf_dir,
            "demo",
            Some(DesiredAppConfig {
                domains: &domains,
                upstreams: &upstreams,
                tls: false,
                auth: None,
                maintenance: false,
                maintenance_message: None,
                redirects: &[],
            }),
        )
        .await;

        assert!(result.is_err(), "apply should fail when validation fails");
        let restored = std::fs::read(app_config_path(&conf_dir, "demo"))
            .expect("config should still exist after rollback");
        assert_eq!(restored, previous, "previous config should be restored");

        std::env::remove_var("DEKU_ANGIE_BIN");
        std::env::remove_var("DEKU_ANGIE_PID_PATH");
    }
}
