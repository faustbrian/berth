//! display about module.
//!
//! Contains display about logic used by Berth command workflows.

use colored::Colorize;
use std::path::Path;

use super::about_style::{AboutRow, print_section};
use crate::{config, docker};

pub(crate) fn print_about(
    config: &config::Config,
    project_root: &Path,
    config_path: &Path,
    runtime_env: Option<&str>,
) {
    let app_name = project_root
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("unknown");
    let runtime = runtime_env.unwrap_or("default");

    let application_rows = [
        AboutRow::plain("Application Name", app_name),
        AboutRow::plain("Berth Version", env!("CARGO_PKG_VERSION")),
        AboutRow::plain("Runtime Environment", runtime),
        AboutRow::plain("Project Root", project_root.display().to_string()),
        AboutRow::plain("Config File", config_path.display().to_string()),
        AboutRow::colored(
            "Dry Run",
            if docker::is_dry_run() {
                "enabled"
            } else {
                "disabled"
            },
            render_enabled_disabled(docker::is_dry_run()),
        ),
    ];
    print_section("Application", &application_rows);

    let container_prefix = config.container_prefix.as_deref().unwrap_or("<none>");
    let services_rows = build_services_rows(config);
    let environment_rows = [
        AboutRow::plain("Container Prefix", container_prefix),
        AboutRow::plain("Configured Services", config.service.len().to_string()),
        AboutRow::plain("Swarm Targets", config.swarm.len().to_string()),
    ];
    print_section("Environment", &environment_rows);
    print_section("Services", &services_rows);

    if !config.swarm.is_empty() {
        let mut swarm_rows = Vec::with_capacity(config.swarm.len() + 1);
        swarm_rows.push(AboutRow::plain(
            "Configured Targets",
            config.swarm.len().to_string(),
        ));
        for target in &config.swarm {
            swarm_rows.push(AboutRow::plain(
                target.name.as_str(),
                target.root.display().to_string(),
            ));
        }
        print_section("Swarm", &swarm_rows);
    }
}

/// Builds services rows for command execution.
fn build_services_rows(config: &config::Config) -> Vec<AboutRow<'static>> {
    let mut running = 0usize;
    let mut created = 0usize;
    let mut exited = 0usize;
    let mut other = 0usize;
    let mut not_created = 0usize;

    for service in &config.service {
        let status =
            docker::inspect_service_status(service).unwrap_or_else(|| "not created".to_owned());
        match status.as_str() {
            "running" => running += 1,
            "created" => created += 1,
            "exited" | "dead" => exited += 1,
            "not created" => not_created += 1,
            _ => other += 1,
        }
    }

    vec![
        AboutRow::plain("Total", config.service.len().to_string()),
        AboutRow::colored(
            "Running",
            running.to_string(),
            running.to_string().green().bold().to_string(),
        ),
        AboutRow::colored(
            "Created",
            created.to_string(),
            created.to_string().yellow().bold().to_string(),
        ),
        AboutRow::colored(
            "Exited",
            exited.to_string(),
            exited.to_string().red().bold().to_string(),
        ),
        AboutRow::colored(
            "Not Created",
            not_created.to_string(),
            not_created.to_string().red().bold().to_string(),
        ),
        AboutRow::plain("Other States", other.to_string()),
    ]
}

/// Renders enabled disabled for command execution.
fn render_enabled_disabled(enabled: bool) -> String {
    if enabled {
        "enabled".green().bold().to_string()
    } else {
        "disabled".red().bold().to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::config::{Config, Driver, Kind, ServiceConfig, SwarmTarget};

    use super::print_about;
    use crate::display::about::render_enabled_disabled;

    fn service(name: &str, kind: Kind, driver: Driver) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind,
            driver,
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

    fn config() -> Config {
        Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![service("app", Kind::App, Driver::Frankenphp)],
            swarm: vec![SwarmTarget {
                name: "stack".to_owned(),
                root: Path::new("/tmp").to_path_buf(),
                depends_on: Vec::new(),
                inject_env: Vec::new(),
                git: None,
            }],
        }
    }

    #[test]
    fn print_about_runs_with_swarm_config() {
        let project_root = Path::new("/tmp");
        let config = config();
        let config_path = Path::new("/tmp/.berth.toml");
        print_about(&config, project_root, config_path, Some("test"));
    }

    #[test]
    fn print_about_runs_without_runtime() {
        let project_root = Path::new("/tmp");
        let config = Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: Some("app-".to_owned()),
            domain_strategy: None,
            service: Vec::new(),
            swarm: Vec::new(),
        };
        let config_path = Path::new("/tmp/.berth.toml");
        print_about(&config, project_root, config_path, None);
    }

    #[test]
    fn render_enabled_disabled_returns_colored_text() {
        assert!(render_enabled_disabled(true).contains("enabled"));
        assert!(render_enabled_disabled(false).contains("disabled"));
    }
}
