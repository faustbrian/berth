//! cli handlers exec cmd module.
//!
//! Contains cli handlers exec cmd logic used by Berth command workflows.

use anyhow::Result;

use super::service_scope::selected_services_in_scope;
use crate::{cli, config, docker};

/// Handles the `exec` CLI command.
pub(crate) struct HandleExecOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) kind: Option<config::Kind>,
    pub(crate) profile: Option<&'a str>,
    pub(crate) non_interactive: bool,
    pub(crate) tty: bool,
    pub(crate) no_tty: bool,
    pub(crate) command: &'a [String],
}

pub(crate) fn handle_exec(config: &config::Config, options: HandleExecOptions<'_>) -> Result<()> {
    let svc = if options.profile.is_some() || options.kind.is_some() || options.service.is_some() {
        let mut selected = selected_services_in_scope(
            config,
            options.service,
            &[],
            options.kind,
            options.profile,
        )?;
        if selected.is_empty() {
            anyhow::bail!("no services matched the requested selector")
        }
        if selected.len() > 1 {
            anyhow::bail!("selector matched multiple services; use --service to choose one")
        }
        selected.remove(0)
    } else if let Ok(app) = config::resolve_app_service(config, None) {
        app
    } else {
        config::resolve_service(config, None)?
    };

    let effective_tty = if options.non_interactive {
        false
    } else {
        cli::support::effective_tty(options.tty, options.no_tty)
    };

    if options.command.is_empty() {
        return docker::pass_through_exit_code(docker::exec_interactive(svc, effective_tty));
    }
    docker::pass_through_exit_code(docker::exec_command(svc, options.command, effective_tty))
}

#[cfg(test)]
mod tests {
    use crate::{cli::handlers::exec_cmd::HandleExecOptions, config, docker};

    fn service(
        name: &str,
        kind: config::Kind,
        driver: config::Driver,
        container_name: &str,
    ) -> config::ServiceConfig {
        config::ServiceConfig {
            name: name.to_owned(),
            kind,
            driver,
            image: "service:latest".to_owned(),
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
            container_name: Some(container_name.to_owned()),
            resolved_container_name: None,
            runtime: Default::default(),
        }
    }

    fn config_with_services(services: Vec<config::ServiceConfig>) -> config::Config {
        config::Config {
            schema_version: 1,
            project_type: config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: services,
            swarm: Vec::new(),
        }
    }

    #[test]
    fn handle_exec_interactive_uses_named_service() -> anyhow::Result<()> {
        let config = config_with_services(vec![
            service(
                "app",
                config::Kind::App,
                config::Driver::Frankenphp,
                "app-container",
            ),
            service(
                "db",
                config::Kind::Database,
                config::Driver::Mysql,
                "db-container",
            ),
        ]);

        let result = docker::with_dry_run_lock(|| {
            super::handle_exec(
                &config,
                HandleExecOptions {
                    service: Some("db"),
                    kind: None,
                    profile: None,
                    non_interactive: false,
                    tty: false,
                    no_tty: false,
                    command: &[],
                },
            )
        });
        result
    }

    #[test]
    fn handle_exec_runs_command_for_first_service_when_no_app_is_configured() -> anyhow::Result<()>
    {
        let config = config_with_services(vec![service(
            "db",
            config::Kind::Database,
            config::Driver::Mysql,
            "db-container",
        )]);

        docker::with_dry_run_lock(|| {
            super::handle_exec(
                &config,
                HandleExecOptions {
                    service: None,
                    kind: None,
                    profile: None,
                    non_interactive: false,
                    tty: true,
                    no_tty: true,
                    command: &["select".to_owned()],
                },
            )
            .expect("handled command when no app exists");
        });

        Ok(())
    }

    #[test]
    fn handle_exec_runs_service_command_with_default_app() -> anyhow::Result<()> {
        let config = config_with_services(vec![service(
            "app",
            config::Kind::App,
            config::Driver::Postgres,
            "app-container",
        )]);

        let command = vec!["psql".to_owned(), "status".to_owned()];
        docker::with_dry_run_lock(|| {
            super::handle_exec(
                &config,
                HandleExecOptions {
                    service: None,
                    kind: None,
                    profile: None,
                    non_interactive: false,
                    tty: false,
                    no_tty: true,
                    command: &command,
                },
            )
            .expect("handled explicit command");
        });

        Ok(())
    }
}
