//! Container stop/remove steps for serve lifecycle teardown.

use anyhow::Result;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

mod steps;
use steps::{StepOutcome, run_docker_step};

/// Stops then removes the target serve container.
pub(super) fn remove_container(target: &ServiceConfig) -> Result<()> {
    let container_name = target.container_name()?;

    if crate::docker::is_dry_run() {
        log_remove_dry_run(&target.name, &container_name);
        return Ok(());
    }

    crate::docker::ensure_own_container(&container_name)?;
    let stop_result = run_docker_step(["stop", &container_name], "stop", &container_name)?;
    let rm_result = run_docker_step(["rm", &container_name], "remove", &container_name)?;

    if matches!(stop_result, StepOutcome::Missing) && matches!(rm_result, StepOutcome::Missing) {
        output::event(
            &target.name,
            LogLevel::Info,
            &format!("Serve container {container_name} is already absent"),
            Persistence::Persistent,
        );
        return Ok(());
    }

    output::event(
        &target.name,
        LogLevel::Success,
        &format!("Stopped and removed serve container {container_name}"),
        Persistence::Persistent,
    );
    Ok(())
}

fn log_remove_dry_run(service_name: &str, container_name: &str) {
    let stop_args = vec!["stop".to_owned(), container_name.to_owned()];
    let rm_args = vec!["rm".to_owned(), container_name.to_owned()];
    output::event(
        service_name,
        LogLevel::Info,
        &format!(
            "[dry-run] {}",
            crate::docker::runtime_command_text(&stop_args)
        ),
        Persistence::Transient,
    );
    output::event(
        service_name,
        LogLevel::Info,
        &format!(
            "[dry-run] {}",
            crate::docker::runtime_command_text(&rm_args)
        ),
        Persistence::Transient,
    );
}

#[cfg(test)]
mod tests {
    use super::remove_container;
    use crate::config::{Driver, Kind, ServiceConfig};

    #[test]
    fn remove_container_propagates_docker_command_failures() {
        crate::docker::with_dry_run_state(false, || {
            let mut service = service("app");
            service.container_name = Some("bad\0name".to_owned());

            let error = remove_container(&service).expect_err("expected docker stop/rm failure");
            assert!(
                error.to_string().contains("Failed")
                    || error.to_string().to_ascii_lowercase().contains("nul")
            );
        });
    }

    fn service(name: &str) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind: Kind::App,
            driver: Driver::Frankenphp,
            image: "dunglas/frankenphp:php8.5".to_owned(),
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
            domain: None,
            domains: None,
            resolved_domain: None,
            routes: None,
            container_port: Some(80),
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
            container_name: Some(name.to_owned()),
            resolved_container_name: None,
            runtime: Default::default(),
        }
    }
}
