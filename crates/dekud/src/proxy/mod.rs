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
    /// The environment `domains`, `upstreams`, and `tls` describe. Vhosts for the
    /// other environments are derived from the database.
    pub environment_id: &'a str,
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

/// The environment's `tls_enabled` flag, for a vhost derived from the database.
async fn app_tls_enabled(pool: &sqlx::SqlitePool, app_id: &str) -> Result<bool> {
    Ok(crate::db::queries::get_app_by_id(pool, app_id)
        .await
        .map(|app| app.tls_enabled)
        .unwrap_or(false))
}

/// Build every vhost of an app: production plus one per environment.
///
/// Production keeps the app's own domains. Each environment gets a derived
/// `<app>-<slug>.<global_domain>` hostname, and none is served when no global
/// domain is configured, since there would be no name to route on.
///
/// The caller's environment is taken as given (it may know about containers a
/// deploy has not finished recording); every other environment is read from the
/// database. That asymmetry is deliberate: it lets a deploy write the
/// environment it just rolled out without deriving the others from stale state,
/// while never letting a deploy's upstreams leak into another environment's
/// vhost.
async fn build_vhosts(
    pool: &sqlx::SqlitePool,
    global_domain: Option<&str>,
    app_id: &str,
    app_name: &str,
    desired: &DesiredAppConfig<'_>,
) -> Result<Vec<writer::VhostConfig>> {
    let environments = crate::db::queries::list_environments(pool, app_id).await?;
    let mut vhosts = Vec::new();

    for environment in environments {
        // The caller described this one; the rest come from the database.
        let from_caller = environment.id == desired.environment_id;

        let (key, domains) = if environment.is_production {
            let domains = if from_caller {
                desired.domains.to_vec()
            } else {
                crate::db::queries::list_domain_names(pool, app_id).await?
            };
            (app_name.to_string(), domains)
        } else {
            let Some(global_domain) = global_domain else {
                continue;
            };
            let hostname = format!("{app_name}-{}.{global_domain}", environment.slug);
            (format!("{app_name}-{}", environment.slug), vec![hostname])
        };

        let (upstreams, tls) = if from_caller {
            (desired.upstreams.to_vec(), desired.tls)
        } else {
            let upstreams =
                crate::db::queries::list_web_upstreams(pool, app_id, &environment.id).await?;
            let tls = if environment.is_production {
                app_tls_enabled(pool, app_id).await?
            } else {
                false
            };
            (upstreams, tls)
        };

        vhosts.push(writer::VhostConfig {
            key,
            domains,
            upstreams,
            tls,
        });
    }

    // A vhost with nothing to serve is omitted rather than pointed at nothing.
    vhosts.retain(|vhost| !vhost.domains.is_empty() && !vhost.upstreams.is_empty());
    Ok(vhosts)
}

/// Write every vhost of an app and reload Angie, restoring the previous config
/// when it will not load.
///
/// `desired` describes one environment; the others are derived from the
/// database. Passing `None` removes the app's config and credential file.
pub async fn apply_app_config(
    pool: &sqlx::SqlitePool,
    conf_dir: &Path,
    global_domain: Option<&str>,
    app_id: &str,
    app_name: &str,
    desired: Option<DesiredAppConfig<'_>>,
) -> Result<()> {
    let previous = read_app_config(conf_dir, app_name)?;

    let vhosts = match &desired {
        Some(config) => build_vhosts(pool, global_domain, app_id, app_name, config).await?,
        None => Vec::new(),
    };

    if vhosts.is_empty() {
        remove_app_config(conf_dir, app_name)?;
    } else {
        let config = desired
            .as_ref()
            .expect("vhosts are only built from a desired config");
        write_app_config(
            conf_dir,
            app_name,
            &vhosts,
            &writer::AppVhostPolicy {
                auth: config.auth,
                maintenance: config.maintenance,
                maintenance_message: config.maintenance_message,
                redirects: config.redirects,
            },
        )?;
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

    /// A migrated pool holding one app with a production environment.
    async fn pool_with_environment(app_id: &str) -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        crate::db::migrate(&pool).await.expect("migrate");
        sqlx::query(
            "INSERT INTO apps (id, name, status, created_at) \
             VALUES (?1, 'demo', 'created', CURRENT_TIMESTAMP)",
        )
        .bind(app_id)
        .execute(&pool)
        .await
        .expect("insert app");
        sqlx::query(
            "INSERT INTO environments (id, app_id, name, slug, is_production, created_at) \
             VALUES ('env-prod', ?1, 'production', 'production', 1, CURRENT_TIMESTAMP)",
        )
        .bind(app_id)
        .execute(&pool)
        .await
        .expect("insert environment");
        pool
    }

    /// Seed an environment with a live deployment and one running web replica.
    async fn seed_environment(
        pool: &sqlx::SqlitePool,
        app_id: &str,
        slug: &str,
        is_production: bool,
        port: i64,
    ) -> String {
        let environment_id = format!("env-{slug}");
        let deployment_id = format!("dep-{slug}");
        sqlx::query(
            "INSERT INTO environments (id, app_id, name, slug, is_production, created_at) \
             VALUES (?1, ?2, ?3, ?3, ?4, CURRENT_TIMESTAMP)",
        )
        .bind(&environment_id)
        .bind(app_id)
        .bind(slug)
        .bind(is_production)
        .execute(pool)
        .await
        .expect("insert environment");
        sqlx::query(
            "INSERT INTO deployments (id, app_id, environment_id, status, builder, image_tag, created_at) \
             VALUES (?1, ?2, ?3, 'live', 'dockerfile', 'deku/x:1', CURRENT_TIMESTAMP)",
        )
        .bind(&deployment_id)
        .bind(app_id)
        .bind(&environment_id)
        .execute(pool)
        .await
        .expect("insert deployment");
        sqlx::query(
            "INSERT INTO containers (id, app_id, deployment_id, process_type, status, host_port, created_at) \
             VALUES (?1, ?2, ?3, 'web', 'running', ?4, CURRENT_TIMESTAMP)",
        )
        .bind(format!("c-{slug}"))
        .bind(app_id)
        .bind(&deployment_id)
        .bind(port)
        .execute(pool)
        .await
        .expect("insert container");
        environment_id
    }

    async fn pool_with_app_and_domain(app_id: &str) -> sqlx::SqlitePool {
        let pool = pool_with_environment(app_id).await;
        sqlx::query("DELETE FROM environments WHERE id = 'env-prod'")
            .execute(&pool)
            .await
            .expect("clear placeholder environment");
        sqlx::query(
            "INSERT INTO domains (id, app_id, domain, created_at) \
             VALUES ('d-1', ?1, 'example.com', CURRENT_TIMESTAMP)",
        )
        .bind(app_id)
        .execute(&pool)
        .await
        .expect("insert domain");
        pool
    }

    #[tokio::test]
    async fn environment_vhosts_are_derived_from_the_database() {
        let pool = pool_with_app_and_domain("app-1").await;
        let production = seed_environment(&pool, "app-1", "production", true, 3000).await;
        seed_environment(&pool, "app-1", "staging", false, 3100).await;

        let desired = DesiredAppConfig {
            environment_id: &production,
            domains: &[String::from("example.com")],
            upstreams: &[Upstream {
                host: "127.0.0.1".to_string(),
                port: 3000,
            }],
            tls: false,
            auth: None,
            maintenance: false,
            maintenance_message: None,
            redirects: &[],
        };

        let vhosts = super::build_vhosts(&pool, Some("apps.test"), "app-1", "demo", &desired)
            .await
            .expect("vhosts");

        assert_eq!(vhosts.len(), 2, "production plus one environment");
        let staging = vhosts
            .iter()
            .find(|vhost| vhost.key == "demo-staging")
            .expect("staging vhost");
        assert_eq!(
            staging.domains,
            vec![String::from("demo-staging.apps.test")],
            "environment hostname is derived from the app, slug, and global domain"
        );
        assert_eq!(
            staging.upstreams.iter().map(|u| u.port).collect::<Vec<_>>(),
            vec![3100],
            "environment vhost must serve its own replica"
        );
        assert!(!staging.tls, "a generated hostname has no certificate");
    }

    #[tokio::test]
    async fn a_deploys_upstreams_never_land_in_another_environments_vhost() {
        let pool = pool_with_app_and_domain("app-1").await;
        seed_environment(&pool, "app-1", "production", true, 3000).await;
        let staging = seed_environment(&pool, "app-1", "staging", false, 3100).await;

        // A staging deploy passes the staging environment with the replica it
        // just started; production must still describe production.
        let desired = DesiredAppConfig {
            environment_id: &staging,
            domains: &[String::from("demo-staging.apps.test")],
            upstreams: &[Upstream {
                host: "127.0.0.1".to_string(),
                port: 9999,
            }],
            tls: false,
            auth: None,
            maintenance: false,
            maintenance_message: None,
            redirects: &[],
        };

        let vhosts = super::build_vhosts(&pool, Some("apps.test"), "app-1", "demo", &desired)
            .await
            .expect("vhosts");

        let production = vhosts
            .iter()
            .find(|vhost| vhost.key == "demo")
            .expect("production vhost");
        assert_eq!(
            production
                .upstreams
                .iter()
                .map(|u| u.port)
                .collect::<Vec<_>>(),
            vec![3000],
            "production must keep serving production's replica"
        );
        assert_eq!(
            production.domains,
            vec![String::from("example.com")],
            "production keeps the app's own domains"
        );

        let staging_vhost = vhosts
            .iter()
            .find(|vhost| vhost.key == "demo-staging")
            .expect("staging vhost");
        assert_eq!(
            staging_vhost
                .upstreams
                .iter()
                .map(|u| u.port)
                .collect::<Vec<_>>(),
            vec![9999],
            "staging serves the replica the caller reported"
        );
    }

    #[tokio::test]
    async fn no_environment_vhost_without_a_global_domain() {
        let pool = pool_with_app_and_domain("app-1").await;
        let production = seed_environment(&pool, "app-1", "production", true, 3000).await;
        seed_environment(&pool, "app-1", "staging", false, 3100).await;

        let desired = DesiredAppConfig {
            environment_id: &production,
            domains: &[String::from("example.com")],
            upstreams: &[Upstream {
                host: "127.0.0.1".to_string(),
                port: 3000,
            }],
            tls: false,
            auth: None,
            maintenance: false,
            maintenance_message: None,
            redirects: &[],
        };

        let vhosts = super::build_vhosts(&pool, None, "app-1", "demo", &desired)
            .await
            .expect("vhosts");

        assert_eq!(
            vhosts.len(),
            1,
            "without a global domain there is no name to route an environment on"
        );
        assert_eq!(vhosts[0].key, "demo");
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

        let pool = pool_with_environment("app-1").await;
        let upstreams = vec![Upstream {
            host: "127.0.0.1".to_string(),
            port: 8080,
        }];
        let domains = vec![String::from("example.com")];
        let result = apply_app_config(
            &pool,
            &conf_dir,
            None,
            "app-1",
            "demo",
            Some(DesiredAppConfig {
                environment_id: "env-prod",
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
