//! Shared app runtime context resolution for CLI handlers.

use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::{config, swarm};

/// Immutable app runtime data needed by service command handlers.
pub(crate) struct AppRuntimeContext<'a> {
    pub(crate) target: &'a config::ServiceConfig,
    pub(crate) workspace_root: PathBuf,
    pub(crate) app_env: HashMap<String, String>,
}

impl AppRuntimeContext<'_> {
    pub(crate) fn service_start_context(&self) -> super::ServiceStartContext<'_> {
        super::ServiceStartContext::new(&self.workspace_root, &self.app_env)
    }
}

/// Resolves app target, workspace root, and inferred env for handlers.
pub(crate) fn resolve_app_runtime_context<'a>(
    config: &'a config::Config,
    service: Option<&str>,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<AppRuntimeContext<'a>> {
    resolve_app_runtime_context_with_workspace_root(
        config,
        service,
        super::workspace_root(config_path, project_root)?,
    )
}

/// Resolves app target with caller-provided workspace root.
///
/// Use this when workspace root was already resolved upstream to avoid duplicate
/// path resolution and potential drift across adjacent runtime steps.
pub(crate) fn resolve_app_runtime_context_with_workspace_root<'a>(
    config: &'a config::Config,
    service: Option<&str>,
    workspace_root: PathBuf,
) -> Result<AppRuntimeContext<'a>> {
    resolve_context(config, service, workspace_root, None)
}

/// [`resolve_app_runtime_context_with_workspace_root`] for `artisan test`,
/// whose `--env=testing` makes Laravel load `.env.testing`: the managed key
/// is decided from that file, so a key it sets is never overridden by the
/// exported environment.
pub(crate) fn resolve_artisan_test_runtime_context<'a>(
    config: &'a config::Config,
    service: Option<&str>,
    workspace_root: PathBuf,
) -> Result<AppRuntimeContext<'a>> {
    resolve_context(config, service, workspace_root, Some("testing"))
}

fn resolve_context<'a>(
    config: &'a config::Config,
    service: Option<&str>,
    workspace_root: PathBuf,
    dotenv_env: Option<&str>,
) -> Result<AppRuntimeContext<'a>> {
    let runtime_env = config
        .service
        .iter()
        .find_map(|service| service.runtime.env.as_deref());
    let project_dependency_env =
        swarm::resolve_project_dependency_injected_env(&workspace_root, runtime_env)?;
    Ok(AppRuntimeContext {
        target: config::resolve_app_service(config, service)?,
        workspace_root,
        app_env: super::runtime_app_env_for_dotenv(config, &project_dependency_env, dotenv_env)?,
    })
}

#[cfg(test)]
mod tests {
    use super::{resolve_app_runtime_context, resolve_app_runtime_context_with_workspace_root};
    use crate::config::{Config, Driver, Kind, ServiceConfig};
    use std::fs;
    use std::path::Path;

    fn app_service() -> ServiceConfig {
        ServiceConfig {
            name: "app".to_owned(),
            kind: Kind::App,
            driver: Driver::Frankenphp,
            image: "php:8.4".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 8080,
            database: None,
            username: None,
            password: None,
            bucket: None,
            access_key: None,
            secret_key: None,
            api_key: None,
            region: None,
            scheme: None,
            domain: Some("app.test".to_owned()),
            domains: None,
            resolved_domain: None,
            routes: None,
            container_port: None,
            smtp_port: None,
            volumes: None,
            env: None,
            command: None,
            depends_on: None,
            seed_file: None,
            hook: Vec::new(),
            health_path: None,
            health_statuses: None,
            restart: None,
            localhost_tls: false,
            octane: false,
            octane_workers: None,
            octane_max_requests: None,
            php_extensions: None,
            trust_container_ca: false,
            env_mapping: None,
            javascript: None,
            container_name: Some("app".to_owned()),
            resolved_container_name: Some("app".to_owned()),
            runtime: Default::default(),
        }
    }

    fn config() -> Config {
        Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![app_service()],
            swarm: Vec::new(),
        }
    }

    #[test]
    fn resolve_app_runtime_context_with_workspace_root_builds_context() {
        let cfg = config();
        let workspace_root = crate::test_support::temp_root().join(format!(
            "berth-workspace-root-{}",
            crate::test_support::unique_stamp()
        ));
        fs::create_dir_all(&workspace_root).expect("create workspace root");
        crate::config::project_file::write_test_config_file(
            workspace_root.join(".berth.yaml"),
            "schema_version = 1\nproject_type = \"project\"\nservice = []\nswarm = []\n",
        )
        .expect("seed workspace config");
        let context = resolve_app_runtime_context_with_workspace_root(
            &cfg,
            Some("app"),
            workspace_root.clone(),
        )
        .expect("runtime context");

        assert_eq!(context.target.name, "app");
        assert_eq!(context.workspace_root, workspace_root);
        assert_eq!(
            context.app_env.get("BERTH_SQL_CLIENT_FLAVOR"),
            Some(&"mysql".to_owned())
        );
        fs::remove_dir_all(workspace_root).expect("cleanup workspace root");
    }

    #[test]
    fn resolve_app_runtime_context_uses_config_path() -> anyhow::Result<()> {
        let dir = crate::test_support::temp_root().join(format!(
            "berth-app-runtime-context-{}",
            crate::test_support::unique_stamp()
        ));
        drop(fs::remove_dir_all(&dir));
        fs::create_dir_all(&dir).expect("create temp workspace");
        let path = dir.join(".berth.yaml");
        crate::config::project_file::write_test_config_file(
            &path,
            "schema_version = 1\nproject_type = \"project\"\n",
        )
        .expect("write config");

        let cfg = config();
        let context = resolve_app_runtime_context(&cfg, None, Some(&path), None)
            .expect("resolve app context");

        assert_eq!(context.target.name, "app");
        assert_eq!(context.workspace_root, dir);
        assert_eq!(
            context.service_start_context().workspace_root,
            Path::new(&dir)
        );
        assert_eq!(
            context.app_env.get("BERTH_SQL_CLIENT_FLAVOR"),
            Some(&"mysql".to_owned())
        );
        Ok(())
    }

    #[test]
    fn resolve_app_runtime_context_with_workspace_root_merges_swarm_injected_env()
    -> anyhow::Result<()> {
        let workspace_root = crate::test_support::temp_root().join(format!(
            "berth-app-runtime-context-injected-env-{}",
            crate::test_support::unique_stamp()
        ));
        let location_root = workspace_root.join("location");
        fs::create_dir_all(&location_root).expect("create dependency workspace");
        crate::config::project_file::write_test_config_file(
            workspace_root.join(".berth.yaml"),
            r#"
project_type = "project"
[[swarm]]
name = "api"
root = "./"
depends_on = ["location"]

[[swarm.inject_env]]
env = "LOCATION_API_BASE_URL"
from = "location"
value = ":base_url"

[[swarm]]
name = "location"
root = "location"
"#,
        )
        .expect("write workspace config");
        crate::config::project_file::write_test_config_file(
            location_root.join(".berth.yaml"),
            r#"
project_type = "project"
container_prefix = "location"

[[service]]
preset = "laravel"
domain = "location.berth.localhost"
"#,
        )
        .expect("write location config");

        let cfg = config();
        let context = resolve_app_runtime_context_with_workspace_root(
            &cfg,
            Some("app"),
            workspace_root.clone(),
        )
        .expect("runtime context");

        // The source instance's own app container on its network, never a
        // route the consumer's container cannot call.
        let url = context
            .app_env
            .get("LOCATION_API_BASE_URL")
            .expect("injected url");
        assert!(
            url.starts_with("http://berth_location_location-") && url.ends_with("_app:80"),
            "{url}"
        );

        fs::remove_dir_all(workspace_root).expect("cleanup workspace");
        Ok(())
    }
}
