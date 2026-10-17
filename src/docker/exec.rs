//! docker exec module.
//!
//! Contains docker exec logic used by Berth command workflows.

use anyhow::Result;
use std::process::Child;

use crate::config::ServiceConfig;
use crate::docker::command_failed_in_container;

use super::{is_dry_run, print_docker_command};
pub(crate) use args::build_exec_args;
use args::{build_client_exec_args, client_env, interactive_client_args, piped_client_args};
use run::{dry_run_process, run_docker_status, spawn_docker_piped};

mod args;
mod run;

/// Execs interactive as part of the docker exec workflow.
pub fn exec_interactive(service: &ServiceConfig, tty: bool) -> Result<()> {
    let container_name = service.container_name()?;
    let client_args = interactive_client_args(service);
    let args = build_client_exec_args(&container_name, &client_env(service), &client_args, tty);

    if is_dry_run() {
        print_docker_command(&args);
        return Ok(());
    }

    ensure_container_running(service, &container_name)?;
    let status = run_docker_status(&args, &super::runtime_command_error_context("exec"))?;

    if !status.success() {
        return Err(super::command_failed_error(
            status,
            command_failed_in_container(&container_name, status.code()),
        ));
    }

    Ok(())
}

/// Fails early, as a berth error, when the service has no running container,
/// so a missing or stopped instance is never reported as the command's own
/// non-zero exit.
fn ensure_container_running(service: &ServiceConfig, container_name: &str) -> Result<()> {
    match super::inspect_status(container_name).as_deref() {
        Some("running") => Ok(()),
        Some(status) => anyhow::bail!(
            "service '{}' is {status} (container {container_name}); run `berth up` first",
            service.name
        ),
        None => anyhow::bail!(
            "service '{}' has no container {container_name}, or Docker is unreachable; run `berth up` first",
            service.name
        ),
    }
}

/// Execs piped as part of the docker exec workflow.
pub fn exec_piped(service: &ServiceConfig, tty: bool) -> Result<Child> {
    let container_name = service.container_name()?;
    let client_args = piped_client_args(service)?;
    let args = build_client_exec_args(&container_name, &client_env(service), &client_args, tty);

    if is_dry_run() {
        print_docker_command(&args);
        return dry_run_process();
    }

    spawn_docker_piped(&args)
}

/// Runs `command` in the service container with its stdout sent to berth's
/// stderr: for berth's own steps (hooks), whose output must never mix into
/// a command's machine-readable stdout.
pub fn exec_command_to_stderr(service: &ServiceConfig, command: &[String]) -> Result<()> {
    let container_name = service.container_name()?;
    let args = build_exec_args(&container_name, command, false);

    if is_dry_run() {
        print_docker_command(&args);
        return Ok(());
    }

    ensure_container_running(service, &container_name)?;
    let arg_refs = super::docker_arg_refs(&args);
    let status = super::secret_env::docker_process(&arg_refs)?
        .stdout(std::io::stderr())
        .status()
        .map_err(|error| anyhow::anyhow!("Failed to execute command in container: {error}"))?;
    if !status.success() {
        return Err(super::command_failed_error(
            status,
            command_failed_in_container(&container_name, status.code()),
        ));
    }
    Ok(())
}

/// Execs command as part of the docker exec workflow.
pub fn exec_command(service: &ServiceConfig, command: &[String], tty: bool) -> Result<()> {
    let container_name = service.container_name()?;
    let args = build_exec_args(&container_name, command, tty);

    if is_dry_run() {
        print_docker_command(&args);
        return Ok(());
    }

    ensure_container_running(service, &container_name)?;
    let status = run_docker_status(&args, "Failed to execute command in container")?;

    if !status.success() {
        return Err(super::command_failed_error(
            status,
            command_failed_in_container(&container_name, status.code()),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::exec_command;
    use crate::config::{Driver, Kind, ServiceConfig};
    use crate::docker;
    use std::env;
    use std::fs;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn with_fake_docker<F, T>(script: &str, test: F) -> T
    where
        F: FnOnce() -> T,
    {
        let bin_dir = env::temp_dir().join(format!(
            "berth-fake-docker-exec-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&bin_dir).expect("create temp dir");

        let binary = bin_dir.join("docker");
        let mut file = fs::File::create(&binary).expect("create fake docker");
        writeln!(file, "#!/bin/sh\n{}", script).expect("write script");
        drop(file);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut perms = fs::metadata(&binary).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&binary, perms).expect("chmod");
        }

        let binary = binary.to_string_lossy().to_string();
        let result = docker::with_docker_command(&binary, test);
        fs::remove_dir_all(&bin_dir).ok();
        result
    }

    fn service() -> ServiceConfig {
        ServiceConfig {
            name: "app".to_owned(),
            kind: Kind::App,
            driver: Driver::Frankenphp,
            image: "app".to_owned(),
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
            container_name: Some("acme-app".to_owned()),
            resolved_container_name: Some("acme-app".to_owned()),
            runtime: Default::default(),
        }
    }

    #[test]
    fn exec_command_includes_exit_code_in_failure_message() {
        with_fake_docker(
            "if [ \"$1\" = inspect ]; then printf running; exit 0; fi; exit 23",
            || {
                let error =
                    exec_command(&service(), &["php".to_owned(), "artisan".to_owned()], false)
                        .expect_err("docker exec should fail");

                assert!(error.to_string().contains("acme-app"));
                assert!(error.to_string().contains("exit code: 23"));
            },
        );
    }
}
