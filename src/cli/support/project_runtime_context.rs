//! Shared project runtime context resolution for CLI handlers.

use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::{config, env};

/// Immutable project runtime data needed by service command handlers.
pub(crate) struct ProjectRuntimeContext {
    pub(crate) workspace_root: PathBuf,
    pub(crate) app_env: HashMap<String, String>,
}

impl ProjectRuntimeContext {
    pub(crate) fn service_start_context(&self) -> super::ServiceStartContext<'_> {
        super::ServiceStartContext::new(&self.workspace_root, &self.app_env)
    }
}

/// Resolves workspace root and inferred env for project-scoped handlers.
pub(crate) fn resolve_project_runtime_context(
    config: &config::Config,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<ProjectRuntimeContext> {
    Ok(ProjectRuntimeContext {
        workspace_root: super::workspace_root(config_path, project_root)?,
        app_env: env::inferred_app_env(config),
    })
}

#[cfg(test)]
mod tests {
    use super::resolve_project_runtime_context;
    use crate::config::{Config, Driver, Kind, ServiceConfig};
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn config() -> Config {
        Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![ServiceConfig {
                name: "db".to_owned(),
                kind: Kind::Database,
                driver: Driver::Postgres,
                image: "postgres:16".to_owned(),
                host: "127.0.0.1".to_owned(),
                port: 5432,
                database: None,
                username: None,
                password: None,
                bucket: None,
                access_key: None,
                secret_key: None,
                api_key: None,
                region: None,
                scheme: None,
                domain: None,
                domains: None,
                resolved_domain: None,
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
                container_name: Some("db".to_owned()),
                resolved_container_name: None,
                runtime: Default::default(),
            }],
            swarm: Vec::new(),
        }
    }

    fn temp_config_path() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "berth-project-runtime-context-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        drop(std::fs::remove_dir_all(&path));
        std::fs::create_dir_all(&path).expect("create temp workspace");
        crate::config::project_file::write_test_config_file(
            path.join(".berth.yaml"),
            "schema_version = 1\nproject_type = \"project\"\n",
        )
        .expect("write config");
        path.join(".berth.yaml")
    }

    #[test]
    fn resolve_project_runtime_context_returns_workspace_root_and_env() -> anyhow::Result<()> {
        let cfg = config();
        let config_path = temp_config_path();
        let context = resolve_project_runtime_context(&cfg, Some(&config_path), None)?;
        let workspace_root = config_path.parent().expect("workspace root").to_path_buf();

        assert_eq!(context.workspace_root, workspace_root);
        assert_eq!(
            context.app_env.get("BERTH_SQL_CLIENT_FLAVOR"),
            Some(&"mysql".to_owned())
        );
        assert_eq!(
            context.service_start_context().workspace_root,
            Path::new(&workspace_root)
        );
        Ok(())
    }
}
