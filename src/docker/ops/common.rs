//! docker ops common module.
//!
//! Shared Docker command execution helpers for docker ops.

use anyhow::Result;
use std::process::Output;

use crate::config::ServiceConfig;
use crate::docker::{is_dry_run, print_docker_command};

pub(super) fn run_docker_status(args: &[String], context_message: &str) -> Result<()> {
    run_docker_output(args, context_message)?;
    Ok(())
}

pub(super) fn run_docker_output(args: &[String], context_message: &str) -> Result<Output> {
    if is_dry_run() {
        print_docker_command(args);
        return Ok(Output {
            status: default_success_status(),
            stdout: Vec::new(),
            stderr: Vec::new(),
        });
    }

    let output = crate::docker::run_docker_output_owned(args, context_message)?;

    if !output.status.success() {
        anyhow::bail!(
            "`{}` failed: {}",
            crate::docker::runtime_command_text(args),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(output)
}

fn default_success_status() -> std::process::ExitStatus {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(0)
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(0)
    }
}

pub(super) fn run_simple_container_command(
    service: &ServiceConfig,
    subcommand: &str,
    context_message: &str,
) -> Result<()> {
    let container_name = service.container_name()?;
    crate::docker::ensure_own_container(&container_name)?;
    let args = vec![subcommand.to_owned(), container_name];
    run_docker_status(&args, context_message)
}

pub(super) fn run_container_command_with_optional_flag(
    service: &ServiceConfig,
    subcommand: &str,
    flag: &str,
    value: Option<&str>,
    context_message: &str,
) -> Result<()> {
    let container_name = service.container_name()?;
    crate::docker::ensure_own_container(&container_name)?;
    let mut args = vec![subcommand.to_owned()];
    if let Some(flag_value) = value {
        args.push(flag.to_owned());
        args.push(flag_value.to_owned());
    }
    args.push(container_name);
    run_docker_status(&args, context_message)
}

pub(super) fn push_flag(args: &mut Vec<String>, enabled: bool, flag: &str) {
    if enabled {
        args.push(flag.to_owned());
    }
}

pub(super) fn push_option(args: &mut Vec<String>, flag: &str, value: Option<&str>) {
    if let Some(value) = value {
        args.push(flag.to_owned());
        args.push(value.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Driver, Kind, ServiceConfig};
    use crate::docker;
    use std::fs;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_service() -> ServiceConfig {
        ServiceConfig {
            name: "db".to_owned(),
            kind: Kind::Database,
            driver: Driver::Postgres,
            image: "postgres:16".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 5432,
            database: Some("app".to_owned()),
            username: Some("root".to_owned()),
            password: Some("secret".to_owned()),
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
            container_name: Some("db".to_owned()),
            resolved_container_name: Some("db".to_owned()),
            runtime: Default::default(),
        }
    }

    fn with_fake_docker<F, T>(script: &str, run: F) -> T
    where
        F: FnOnce() -> T,
    {
        let bin_dir = crate::test_support::temp_root().join(format!(
            "berth-ops-common-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        // A clock tick is coarse: two parallel tests could share a name and
        // remove each other's fake binary.
        let bin_dir = docker::unique_test_path(bin_dir);
        fs::create_dir_all(&bin_dir).expect("temp dir");
        let binary = bin_dir.join("docker");
        let mut file = fs::File::create(&binary).expect("write fake docker");
        writeln!(file, "#!/bin/sh\n{}", script).expect("script");
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&binary).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&binary, perms).expect("chmod");
        }

        let command = binary.to_string_lossy().to_string();
        let result = docker::with_docker_command(&command, run);
        fs::remove_dir_all(&bin_dir).ok();
        result
    }

    #[test]
    fn push_flag_adds_value_when_enabled() {
        let mut args = vec!["run".to_owned()];
        push_flag(&mut args, true, "--rm");
        assert_eq!(args, vec!["run", "--rm"]);
    }

    #[test]
    fn push_flag_skips_when_disabled() {
        let mut args = vec!["run".to_owned()];
        push_flag(&mut args, false, "--rm");
        assert_eq!(args, vec!["run"]);
    }

    #[test]
    fn push_option_adds_pair_when_present() {
        let mut args = vec!["run".to_owned()];
        push_option(&mut args, "--name", Some("svc"));
        assert_eq!(args, vec!["run", "--name", "svc"]);
    }

    #[test]
    fn push_option_skips_when_absent() {
        let mut args = vec!["run".to_owned()];
        push_option(&mut args, "--name", None);
        assert_eq!(args, vec!["run"]);
    }

    #[test]
    fn run_docker_status_uses_dry_run_when_enabled() {
        docker::with_dry_run_state(true, || {
            run_simple_container_command(&make_service(), "rm", "remove").expect("dry run");
        });
    }

    #[test]
    fn run_container_command_with_optional_flag_includes_flag_when_set() {
        docker::with_dry_run_state(true, || {
            let service = make_service();
            run_container_command_with_optional_flag(
                &service,
                "inspect",
                "--size",
                Some("true"),
                "inspect",
            )
            .expect("dry run");
        });
    }

    #[test]
    fn run_docker_output_uses_system_docker_binary() {
        docker::with_dry_run_state(false, || {
            with_fake_docker("printf '%s' ok", || {
                let output =
                    run_docker_output(&["ls".to_owned()], "works").expect("status command");
                assert!(output.status.success());
                assert_eq!(String::from_utf8_lossy(&output.stdout), "ok");
            });
        });
    }

    #[test]
    fn run_docker_output_returns_error_for_failure() {
        docker::with_dry_run_state(false, || {
            with_fake_docker("echo 'Error: No such container: x' >&2; exit 1", || {
                let output = run_docker_output(&["fail".to_owned()], "fail").expect_err("failure");
                assert!(output.to_string().contains("` failed"), "{output}");
                assert!(
                    output.to_string().contains("No such container: x"),
                    "the engine's own error reaches the user: {output}"
                );
            });
        });
    }
}
