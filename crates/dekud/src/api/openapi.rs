//! OpenAPI document generation and the Scalar API reference UI.
//!
//! The spec is served at `/api/openapi.json` and rendered by Scalar at
//! `/api/docs`. The Scalar standalone bundle is vendored with the dashboard
//! assets (`dashboard/public/scalar/`) and served same-origin at
//! `/scalar/standalone.js`, so the reference works offline.
//!
//! Handlers are annotated with `#[utoipa::path]` incrementally; `ApiDoc` lists
//! the annotated subset and the completeness test guards that list.

use super::console::{__path_exec, __path_run};
use super::services::{
    __path_cron_add, __path_cron_list, __path_cron_remove, __path_delete_backup_schedule,
    __path_get_backup_schedule, __path_le_config, __path_le_disable, __path_le_enable,
    __path_le_get_config, __path_le_status, __path_list_backup_schedules, __path_my_backup,
    __path_my_backups, __path_my_create, __path_my_destroy, __path_my_info, __path_my_link,
    __path_my_list, __path_my_logs, __path_my_restore, __path_my_unlink, __path_net_attach,
    __path_net_create, __path_net_destroy, __path_net_detach, __path_net_list,
    __path_net_list_for_app, __path_pg_backup, __path_pg_backups, __path_pg_create,
    __path_pg_destroy, __path_pg_info, __path_pg_link, __path_pg_list, __path_pg_logs,
    __path_pg_restore, __path_pg_unlink, __path_rd_backup, __path_rd_backups, __path_rd_create,
    __path_rd_destroy, __path_rd_info, __path_rd_link, __path_rd_list, __path_rd_logs,
    __path_rd_restore, __path_rd_unlink, __path_set_backup_schedule, __path_storage_add,
    __path_storage_ensure, __path_storage_list, __path_storage_remove,
};
use super::services::{
    __path_svc_backup, __path_svc_backups, __path_svc_create, __path_svc_destroy, __path_svc_info,
    __path_svc_link, __path_svc_list, __path_svc_logs, __path_svc_restore, __path_svc_unlink,
    AddCronBody, AddMountBody, CreateNetworkBody, CreateServiceBody, EnsureDirBody, LeConfigBody,
    SetBackupScheduleBody,
};
use super::{
    __path_add_domain, __path_add_port, __path_add_redirect_handler, __path_add_ssh_key,
    __path_check_build_host, __path_clone_app, __path_create_app, __path_create_app_deploy_token,
    __path_create_environment_handler, __path_delete_app, __path_delete_app_auth_handler,
    __path_delete_environment_handler, __path_deploy_archive, __path_doctor, __path_get_app,
    __path_get_app_auth_handler, __path_get_app_checks, __path_get_app_object_store_link,
    __path_get_build_host, __path_get_limits, __path_get_logs, __path_get_maintenance,
    __path_get_object_store_config, __path_get_plugins_runtime, __path_get_registry,
    __path_get_routing_status, __path_get_routing_status_for_app, __path_get_scale,
    __path_get_version_status, __path_health_check, __path_import_app_config,
    __path_init_build_host, __path_install_plugin, __path_link_app_object_store,
    __path_list_alerts_handler, __path_list_app_deploy_tokens, __path_list_apps,
    __path_list_config, __path_list_deployments, __path_list_domains,
    __path_list_environments_handler, __path_list_events, __path_list_plugins, __path_list_ports,
    __path_list_processes, __path_list_redirects_handler, __path_list_routing,
    __path_list_ssh_keys, __path_metrics_handler, __path_remove_domain, __path_remove_port,
    __path_remove_redirect_handler, __path_remove_ssh_key, __path_rename_app,
    __path_revoke_app_deploy_token, __path_rotate_dashboard_token, __path_set_app_auth_handler,
    __path_set_build_host, __path_set_config, __path_set_limits, __path_set_maintenance,
    __path_set_object_store_config, __path_set_registry, __path_set_scale,
    __path_stream_app_events, __path_stream_events, __path_test_object_store_config,
    __path_trigger_deploy, __path_trigger_rollback, __path_uninstall_plugin,
    __path_unlink_app_object_store, __path_unset_build_host, __path_unset_config,
    __path_unset_object_store_config, __path_unset_registry, __path_update_routing,
    __path_verify_dashboard_session,
};
use super::{
    AddDomainBody, AddPortBody, AddRedirectBody, AddSshKeyBody, CreateDeployTokenBody, DeployBody,
    ImportConfigBody, InstallPluginBody, ObjectStoreAppLinkBody, RenameAppBody, RollbackBody,
    SetConfigBody, SetLimitsBody, SetMaintenanceBody, SetScaleBody,
};
use axum::response::{Html, IntoResponse};
use utoipa::OpenApi;

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct HealthSchema {
    /// Always `ok` when the daemon is serving.
    pub status: String,
    /// Service identifier, `dekud`.
    pub service: String,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct AppSchema {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub locked: bool,
    pub status: String,
    pub tls_enabled: bool,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct NewAppSchema {
    pub name: String,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct ConsoleCommandSchema {
    /// Command and arguments, e.g. `["python", "manage.py", "migrate"]`.
    pub command: Vec<String>,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct SetAppAuthSchema {
    /// "basic" or "forward".
    pub mode: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub forward_url: Option<String>,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct AppAuthStatusSchema {
    pub configured: bool,
    pub mode: Option<String>,
    pub username: Option<String>,
    pub forward_url: Option<String>,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct MaintenanceSchema {
    pub enabled: bool,
    pub message: Option<String>,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct RedirectSchema {
    pub id: String,
    pub source_path: String,
    pub target: String,
    pub code: i64,
    pub created_at: String,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct AddRedirectSchema {
    pub source_path: String,
    pub target: String,
    pub code: Option<i64>,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct LimitsSchema {
    pub process_type: Option<String>,
    pub cpu: Option<String>,
    pub memory: Option<String>,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct DoctorCheckSchema {
    pub name: String,
    pub status: String,
    pub detail: String,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct BuildHostSchema {
    pub name: String,
    pub host: String,
    pub identity_file: Option<String>,
    pub buildkit_host: String,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct RegistrySchema {
    pub server: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub namespace: String,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct BuildHostStatusSchema {
    pub configured: bool,
    pub build_host: Option<BuildHostSchema>,
    pub registry: Option<RegistrySchema>,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct RegistryStatusSchema {
    pub configured: bool,
    pub registry: Option<RegistrySchema>,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct BuildHostCheckSchema {
    pub ok: bool,
    pub checks: Vec<DoctorCheckSchema>,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct BuildHostInitSchema {
    pub ok: bool,
    pub notes: Vec<String>,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct UpstreamSchema {
    pub host: String,
    pub port: u16,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct RoutingBodySchema {
    pub upstreams: Vec<UpstreamSchema>,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct ObjectStoreConfigSchema {
    pub provider: String,
    pub bucket: String,
    pub region: String,
    pub endpoint: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub path_style: bool,
    pub prefix: Option<String>,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct DeployTokenSchema {
    pub id: String,
    pub name: String,
    pub prefix: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct NewDeployTokenSchema {
    pub id: String,
    pub name: String,
    pub prefix: String,
    pub created_at: String,
    /// The plaintext token, shown only in this response.
    pub token: String,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct ConfigImportSummarySchema {
    pub created: u64,
    pub overwritten: u64,
    pub skipped: Vec<String>,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct PluginRuntimeSchema {
    /// Whether this dekud binary includes the in-process cdylib runtime.
    pub available: bool,
    /// How to enable it. Present only when `available` is false.
    pub message: Option<String>,
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Deku API",
        version = env!("CARGO_PKG_VERSION"),
        description = "Operator API for a single-server Deku host. TCP requests require a dashboard bearer token; the local Unix socket is trusted access."
    ),
    paths(
        docs,
        spec,
        list_alerts_handler,
        metrics_handler,
        list_environments_handler,
        create_environment_handler,
        delete_environment_handler,
        get_plugins_runtime,
        rename_app,
        clone_app,
        list_app_deploy_tokens,
        create_app_deploy_token,
        revoke_app_deploy_token,
        import_app_config,
        svc_backup,
        svc_backups,
        svc_create,
        svc_destroy,
        svc_info,
        svc_link,
        svc_list,
        svc_logs,
        svc_restore,
        svc_unlink,
        add_domain,
        add_port,
        add_redirect_handler,
        add_ssh_key,
        check_build_host,
        create_app,
        delete_app,
        delete_app_auth_handler,
        deploy_archive,
        doctor,
        get_app,
        get_app_auth_handler,
        get_app_checks,
        get_app_object_store_link,
        get_build_host,
        get_limits,
        get_logs,
        get_maintenance,
        get_object_store_config,
        get_registry,
        get_routing_status,
        get_routing_status_for_app,
        get_scale,
        get_version_status,
        health_check,
        init_build_host,
        install_plugin,
        link_app_object_store,
        list_apps,
        list_config,
        list_deployments,
        list_domains,
        list_events,
        list_plugins,
        list_ports,
        list_processes,
        list_redirects_handler,
        list_routing,
        list_ssh_keys,
        remove_domain,
        remove_port,
        remove_redirect_handler,
        remove_ssh_key,
        rotate_dashboard_token,
        set_app_auth_handler,
        set_build_host,
        set_config,
        set_limits,
        set_maintenance,
        set_object_store_config,
        set_registry,
        set_scale,
        stream_app_events,
        stream_events,
        test_object_store_config,
        trigger_deploy,
        trigger_rollback,
        uninstall_plugin,
        unlink_app_object_store,
        unset_build_host,
        unset_config,
        unset_object_store_config,
        unset_registry,
        update_routing,
        verify_dashboard_session,
        exec,
        run,
        cron_add,
        cron_list,
        cron_remove,
        delete_backup_schedule,
        get_backup_schedule,
        le_config,
        le_disable,
        le_enable,
        le_get_config,
        le_status,
        list_backup_schedules,
        my_backup,
        my_backups,
        my_create,
        my_destroy,
        my_info,
        my_link,
        my_list,
        my_logs,
        my_restore,
        my_unlink,
        net_attach,
        net_create,
        net_destroy,
        net_detach,
        net_list,
        net_list_for_app,
        pg_backup,
        pg_backups,
        pg_create,
        pg_destroy,
        pg_info,
        pg_link,
        pg_list,
        pg_logs,
        pg_restore,
        pg_unlink,
        rd_backup,
        rd_backups,
        rd_create,
        rd_destroy,
        rd_info,
        rd_link,
        rd_list,
        rd_logs,
        rd_restore,
        rd_unlink,
        set_backup_schedule,
        storage_add,
        storage_ensure,
        storage_list,
        storage_remove
    ),
    components(schemas(
        HealthSchema,
        AppSchema,
        NewAppSchema,
        ConsoleCommandSchema,
        SetAppAuthSchema,
        AppAuthStatusSchema,
        MaintenanceSchema,
        RedirectSchema,
        AddRedirectSchema,
        LimitsSchema,
        DoctorCheckSchema,
        BuildHostSchema,
        RegistrySchema,
        BuildHostStatusSchema,
        RegistryStatusSchema,
        BuildHostCheckSchema,
        BuildHostInitSchema,
        PluginRuntimeSchema,
        CreateDeployTokenBody,
        ImportConfigBody,
        RenameAppBody,
        ConfigImportSummarySchema,
        DeployTokenSchema,
        NewDeployTokenSchema,
        UpstreamSchema,
        RoutingBodySchema,
        ObjectStoreConfigSchema,
        ObjectStoreAppLinkBody,
        SetMaintenanceBody,
        AddRedirectBody,
        AddDomainBody,
        AddPortBody,
        DeployBody,
        RollbackBody,
        SetConfigBody,
        SetLimitsBody,
        SetScaleBody,
        AddSshKeyBody,
        InstallPluginBody,
        CreateServiceBody,
        LeConfigBody,
        CreateNetworkBody,
        AddMountBody,
        EnsureDirBody,
        AddCronBody,
        SetBackupScheduleBody
    ))
)]
pub struct ApiDoc;

/// Serves the generated OpenAPI document as JSON, with the dashboard bearer
/// token declared so interactive clients can authenticate.
#[utoipa::path(
    get,
    path = "/api/openapi.json",
    tag = "system",
    responses((status = 200, description = "OpenAPI 3.1 document"))
)]
pub async fn spec() -> impl IntoResponse {
    let mut document = ApiDoc::openapi();
    if let Some(components) = document.components.as_mut() {
        components.add_security_scheme(
            "bearerAuth",
            utoipa::openapi::security::SecurityScheme::Http(utoipa::openapi::security::Http::new(
                utoipa::openapi::security::HttpAuthScheme::Bearer,
            )),
        );
    }
    axum::Json(document)
}

const DOCS_HTML: &str = r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Deku API Reference</title>
  </head>
  <body>
    <div id="app"></div>
    <script src="/scalar/standalone.js"></script>
    <script>
      Scalar.createApiReference('#app', {
        url: '/api/openapi.json',
      });
    </script>
  </body>
</html>
"#;

/// Serves the Scalar API reference UI.
#[utoipa::path(
    get,
    path = "/api/docs",
    tag = "system",
    responses((status = 200, description = "Scalar API reference UI"))
)]
pub async fn docs() -> Html<&'static str> {
    Html(DOCS_HTML)
}

#[cfg(test)]
mod tests {
    use super::ApiDoc;
    use utoipa::OpenApi;

    /// Every annotated handler must appear in the spec. When a new group is
    /// annotated, add its `(method, path)` pairs here so a dropped
    /// `paths(...)` entry fails the build.
    const EXPECTED: &[(&str, &str)] = &[
        ("delete", "/api/apps/{app}/cron/{id}"),
        ("get", "/api/alerts"),
        ("get", "/api/apps/{name}/environments"),
        ("post", "/api/apps/{name}/environments"),
        ("delete", "/api/apps/{name}/environments/{slug}"),
        ("get", "/api/metrics"),
        ("delete", "/api/apps/{app}/networks/{network}"),
        ("delete", "/api/apps/{app}/storage/{id}"),
        ("delete", "/api/apps/{name}"),
        ("delete", "/api/apps/{name}/auth"),
        ("delete", "/api/apps/{name}/config/{key}"),
        ("delete", "/api/apps/{name}/domains/{domain}"),
        ("delete", "/api/apps/{name}/objectstore"),
        ("delete", "/api/apps/{name}/ports/{id}"),
        ("delete", "/api/apps/{name}/redirects/{id}"),
        ("delete", "/api/build-host"),
        ("delete", "/api/mysql/services/{name}"),
        ("delete", "/api/mysql/services/{name}/link/{app}"),
        ("delete", "/api/networks/{name}"),
        ("delete", "/api/objectstore"),
        ("delete", "/api/plugins/{name}"),
        ("delete", "/api/postgres/services/{name}"),
        ("delete", "/api/postgres/services/{name}/link/{app}"),
        ("delete", "/api/redis/services/{name}"),
        ("delete", "/api/redis/services/{name}/link/{app}"),
        ("delete", "/api/registry"),
        ("delete", "/api/services/{name}/backup-schedule"),
        ("delete", "/api/ssh-keys/{name}"),
        ("get", "/api/apps"),
        ("get", "/api/apps/{app}/cron"),
        ("get", "/api/apps/{app}/networks"),
        ("get", "/api/apps/{app}/storage"),
        ("get", "/api/apps/{name}"),
        ("get", "/api/apps/{name}/auth"),
        ("get", "/api/apps/{name}/checks"),
        ("get", "/api/apps/{name}/config"),
        ("get", "/api/apps/{name}/deployments"),
        ("get", "/api/apps/{name}/domains"),
        ("get", "/api/apps/{name}/events/stream"),
        ("get", "/api/apps/{name}/limits"),
        ("get", "/api/apps/{name}/logs"),
        ("get", "/api/apps/{name}/maintenance"),
        ("get", "/api/apps/{name}/objectstore"),
        ("get", "/api/apps/{name}/ports"),
        ("get", "/api/apps/{name}/ps"),
        ("get", "/api/apps/{name}/redirects"),
        ("get", "/api/apps/{name}/scale"),
        ("get", "/api/backup-schedules"),
        ("get", "/api/build-host"),
        ("get", "/api/doctor"),
        ("post", "/api/apps/{name}/clone"),
        ("post", "/api/apps/{name}/rename"),
        ("post", "/api/apps/{name}/config/import"),
        ("get", "/api/apps/{name}/deploy-tokens"),
        ("post", "/api/apps/{name}/deploy-tokens"),
        ("delete", "/api/apps/{name}/deploy-tokens/{id}"),
        ("get", "/api/docs"),
        ("get", "/api/openapi.json"),
        ("get", "/api/events"),
        ("get", "/api/events/stream"),
        ("get", "/api/letsencrypt/config"),
        ("get", "/api/letsencrypt/status/{app}"),
        ("get", "/api/mysql/services"),
        ("get", "/api/mysql/services/{name}"),
        ("get", "/api/mysql/services/{name}/backups"),
        ("get", "/api/mysql/services/{name}/logs"),
        ("get", "/api/networks"),
        ("get", "/api/objectstore"),
        ("get", "/api/plugins"),
        ("get", "/api/plugins/runtime"),
        ("get", "/api/postgres/services"),
        ("get", "/api/postgres/services/{name}"),
        ("get", "/api/postgres/services/{name}/backups"),
        ("get", "/api/postgres/services/{name}/logs"),
        ("get", "/api/redis/services"),
        ("get", "/api/redis/services/{name}"),
        ("get", "/api/redis/services/{name}/backups"),
        ("get", "/api/redis/services/{name}/logs"),
        ("get", "/api/registry"),
        ("get", "/api/routing"),
        ("get", "/api/routing/status"),
        ("get", "/api/routing/status/{name}"),
        ("get", "/api/services/{type}"),
        ("post", "/api/services/{type}"),
        ("get", "/api/services/{type}/{name}"),
        ("delete", "/api/services/{type}/{name}"),
        ("post", "/api/services/{type}/{name}/link/{app}"),
        ("delete", "/api/services/{type}/{name}/link/{app}"),
        ("get", "/api/services/{type}/{name}/logs"),
        ("get", "/api/services/{type}/{name}/backups"),
        ("post", "/api/services/{type}/{name}/backups"),
        ("post", "/api/services/{type}/{name}/restore/{backup_id}"),
        ("get", "/api/services/{name}/backup-schedule"),
        ("get", "/api/ssh-keys"),
        ("get", "/api/version"),
        ("get", "/healthz"),
        ("post", "/api/apps"),
        ("post", "/api/apps/{app}/cron"),
        ("post", "/api/apps/{app}/networks/{network}"),
        ("post", "/api/apps/{app}/storage"),
        ("post", "/api/apps/{app}/storage/ensure"),
        ("post", "/api/apps/{name}/auth"),
        ("post", "/api/apps/{name}/config"),
        ("post", "/api/apps/{name}/deploy"),
        ("post", "/api/apps/{name}/deploy/archive"),
        ("post", "/api/apps/{name}/domains"),
        ("post", "/api/apps/{name}/exec"),
        ("post", "/api/apps/{name}/limits"),
        ("post", "/api/apps/{name}/maintenance"),
        ("post", "/api/apps/{name}/objectstore"),
        ("post", "/api/apps/{name}/ports"),
        ("post", "/api/apps/{name}/redirects"),
        ("post", "/api/apps/{name}/rollback"),
        ("post", "/api/apps/{name}/run"),
        ("post", "/api/apps/{name}/scale"),
        ("post", "/api/build-host"),
        ("post", "/api/build-host/check"),
        ("post", "/api/build-host/init"),
        ("post", "/api/dashboard/session"),
        ("post", "/api/dashboard/token"),
        ("post", "/api/letsencrypt/config"),
        ("post", "/api/letsencrypt/disable/{app}"),
        ("post", "/api/letsencrypt/enable/{app}"),
        ("post", "/api/mysql/services"),
        ("post", "/api/mysql/services/{name}/backups"),
        ("post", "/api/mysql/services/{name}/link/{app}"),
        ("post", "/api/mysql/services/{name}/restore/{backup_id}"),
        ("post", "/api/networks"),
        ("post", "/api/objectstore"),
        ("post", "/api/objectstore/test"),
        ("post", "/api/plugins"),
        ("post", "/api/postgres/services"),
        ("post", "/api/postgres/services/{name}/backups"),
        ("post", "/api/postgres/services/{name}/link/{app}"),
        ("post", "/api/postgres/services/{name}/restore/{backup_id}"),
        ("post", "/api/redis/services"),
        ("post", "/api/redis/services/{name}/backups"),
        ("post", "/api/redis/services/{name}/link/{app}"),
        ("post", "/api/redis/services/{name}/restore/{backup_id}"),
        ("post", "/api/registry"),
        ("post", "/api/routing/{name}"),
        ("post", "/api/services/{name}/backup-schedule"),
        ("post", "/api/ssh-keys"),
    ];

    #[test]
    fn spec_serializes_and_covers_annotated_routes() {
        let value = serde_json::to_value(ApiDoc::openapi()).expect("spec should serialize");

        assert_eq!(value["openapi"], "3.1.0");

        let paths = value["paths"].as_object().expect("paths object");
        for (method, path) in EXPECTED {
            assert!(
                paths
                    .get(*path)
                    .is_some_and(|item| item.get(*method).is_some()),
                "missing {method} {path} in spec"
            );
        }
    }

    /// Collect `(method, path)` pairs from the axum router so a newly registered
    /// route cannot ship without an annotation and an `EXPECTED` entry.
    fn router_routes() -> Vec<(String, String)> {
        let source = include_str!("mod.rs");
        let router: &str = source[source.find("fn build_api_router").expect("router")..]
            .split("fn build_public_router")
            .next()
            .expect("router body");

        let mut pairs = Vec::new();
        let mut rest = router;
        while let Some(index) = rest.find(".route(") {
            rest = &rest[index + ".route(".len()..];
            let mut depth = 1;
            let mut end = 0;
            for (position, ch) in rest.char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = position;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let call = &rest[..end];
            rest = &rest[end..];

            let Some(quote) = call.find('"') else {
                continue;
            };
            let Some(closing) = call[quote + 1..].find('"') else {
                continue;
            };
            let path = &call[quote + 1..quote + 1 + closing];

            for method in ["get", "post", "put", "delete", "patch"] {
                let needle = format!("{method}(");
                if call.contains(&needle) {
                    pairs.push((method.to_string(), path.to_string()));
                }
            }
        }
        pairs
    }

    #[test]
    fn every_router_route_is_documented() {
        let documented: std::collections::HashSet<(&str, &str)> =
            EXPECTED.iter().copied().collect();
        let mut undocumented = Vec::new();
        for (method, path) in router_routes() {
            if !documented.contains(&(method.as_str(), path.as_str())) {
                undocumented.push(format!("{method} {path}"));
            }
        }
        assert!(
            undocumented.is_empty(),
            "routes are missing from the OpenAPI spec: {undocumented:?}"
        );
    }
}
