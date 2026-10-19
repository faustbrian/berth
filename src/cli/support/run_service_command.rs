//! Shared helper to run a command inside a target service container.

use anyhow::Result;

use crate::{config, serve};

pub(crate) fn run_service_command(
    target: &config::ServiceConfig,
    command: &[String],
    context: &super::ServiceStartContext<'_>,
) -> Result<()> {
    run_service_command_with_tty(target, command, false, context)
}

pub(crate) fn run_service_command_with_tty(
    target: &config::ServiceConfig,
    command: &[String],
    tty: bool,
    context: &super::ServiceStartContext<'_>,
) -> Result<()> {
    serve::exec_or_run_command(
        target,
        command,
        tty,
        context.workspace_root,
        context.app_env,
    )
}

pub(crate) fn run_service_commands(
    target: &config::ServiceConfig,
    commands: &[Vec<String>],
    context: &super::ServiceStartContext<'_>,
) -> Result<()> {
    for command in commands {
        run_service_command(target, command, context)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use std::path::PathBuf;

    use crate::config::{Driver, Kind, ServiceConfig};

    use super::run_service_commands;
    use crate::cli::support::ServiceStartContext;

    fn service() -> ServiceConfig {
        ServiceConfig {
            name: "app".to_owned(),
            kind: Kind::App,
            driver: Driver::Frankenphp,
            image: "php".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 9000,
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
            container_name: None,
            resolved_container_name: None,
            runtime: Default::default(),
        }
    }

    #[test]
    fn run_service_commands_skips_empty_command_list() {
        let target = service();
        let workspace_root = PathBuf::from("/tmp");
        let context = ServiceStartContext {
            workspace_root: workspace_root.as_path(),
            app_env: &HashMap::new(),
        };
        assert!(run_service_commands(&target, &Vec::new(), &context).is_ok());
    }
}
