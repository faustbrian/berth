//! doctor app reachability checks.

use crate::cli::support::run_doctor::report;
use crate::{cli, config, docker, serve};

/// Checks public app and health endpoint reachability for a serve target.
pub(in crate::cli::support::run_doctor::app) fn check_http_reachability(
    target: &config::ServiceConfig,
) -> bool {
    let status = target
        .container_name()
        .ok()
        .and_then(|name| docker::inspect_status(&name))
        .unwrap_or_else(|| "not created".to_owned());
    if status != "running" {
        report::error(&format!(
            "App service '{}' is not running (status: {status})",
            target.name
        ));
        return true;
    }

    let app_url = match serve::public_url(target) {
        Ok(url) => url,
        Err(err) => {
            report::error(&format!(
                "App service '{}' URL resolution failed: {err}",
                target.name
            ));
            return true;
        }
    };

    let health_url = cli::support::build_health_url(target, &app_url, None);
    let app_error = check_endpoint(
        target,
        &app_url,
        "app URL",
        |status_code| (200..400).contains(&status_code),
        |status_code, url| format!("{} app URL unhealthy ({status_code}): {url}", target.name),
        |url| format!("{} app URL unreachable: {url}", target.name),
    );
    let health_error = check_endpoint(
        target,
        &health_url,
        "health endpoint",
        |status_code| cli::support::health_status_accepted(target, status_code),
        |status_code, url| {
            format!(
                "{} health endpoint returned {status_code}: {url}",
                target.name
            )
        },
        |url| format!("{} health endpoint unreachable: {url}", target.name),
    );

    app_error || health_error
}

fn check_endpoint<Accept, RejectMsg, UnreachableMsg>(
    target: &config::ServiceConfig,
    url: &str,
    label: &str,
    is_accepted: Accept,
    reject_message: RejectMsg,
    unreachable_message: UnreachableMsg,
) -> bool
where
    Accept: Fn(u16) -> bool,
    RejectMsg: Fn(u16, &str) -> String,
    UnreachableMsg: Fn(&str) -> String,
{
    if let Some(status_code) = cli::support::probe_http_status(url) {
        if is_accepted(status_code) {
            report::success(&format!(
                "{} {label} reachable ({status_code})",
                target.name
            ));
            return false;
        }

        report::error(&reject_message(status_code, url));
        return true;
    }

    report::error(&unreachable_message(url));
    true
}

#[cfg(test)]
mod tests {
    use crate::{
        cli::support::probe_http_status::with_curl_command,
        config::{Driver, Kind, ServiceConfig},
        docker,
    };

    use super::check_http_reachability;

    fn service() -> ServiceConfig {
        ServiceConfig {
            name: "app".to_owned(),
            kind: Kind::App,
            driver: Driver::Frankenphp,
            image: "dunglas/frankenphp:php8.5".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 3300,
            database: None,
            username: None,
            password: None,
            bucket: None,
            access_key: None,
            secret_key: None,
            api_key: None,
            region: None,
            scheme: Some("https".to_owned()),
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
            localhost_tls: true,
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

    fn with_fake_docker_status<F, T>(status: &str, test: F) -> T
    where
        F: FnOnce() -> T,
    {
        let bin_dir = crate::test_support::temp_root().join(format!(
            "berth-doctor-reachability-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&bin_dir).expect("create temp dir");
        let docker_bin = bin_dir.join("docker");
        let mut file = std::fs::File::create(&docker_bin).expect("create fake docker");
        use std::io::Write;
        writeln!(file, "#!/bin/sh\nprintf '{}'; exit 0", status).expect("write script");
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&docker_bin)
                .expect("metadata")
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&docker_bin, perms).expect("chmod");
        }

        let binary = docker_bin.to_string_lossy().to_string();
        let result =
            docker::with_dry_run_state(false, || docker::with_docker_command(&binary, test));
        std::fs::remove_dir_all(&bin_dir).ok();
        result
    }

    fn with_fake_curl_for_urls<F, T>(app_code: u16, health_code: u16, test: F) -> T
    where
        F: FnOnce() -> T,
    {
        let bin_dir = crate::test_support::temp_root().join(format!(
            "berth-doctor-reachability-curl-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&bin_dir).expect("create temp dir");
        let curl = bin_dir.join("curl");
        let mut file = std::fs::File::create(&curl).expect("create fake curl");
        use std::io::Write;
        writeln!(
            file,
            "#!/bin/sh\ncase \"$7\" in\n  *\"/up\")\n    printf '{}';\n    ;;\n  *)\n    printf '{}';\n    ;;\nesac\n",
            health_code,
            app_code
        )
        .expect("write script");
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&curl).expect("metadata").permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&curl, perms).expect("chmod");
        }

        let command = curl.to_string_lossy().to_string();
        let result = with_curl_command(&command, test);
        std::fs::remove_dir_all(&bin_dir).ok();
        result
    }

    #[test]
    fn check_http_reachability_reports_ok_when_endpoints_are_healthy() {
        with_fake_curl_for_urls(200, 200, || {
            with_fake_docker_status("running", || {
                assert!(!check_http_reachability(&service()));
            });
        });
    }

    #[test]
    fn check_http_reachability_reports_error_when_app_is_unhealthy() {
        with_fake_curl_for_urls(500, 200, || {
            with_fake_docker_status("running", || {
                assert!(check_http_reachability(&service()));
            });
        });
    }

    #[test]
    fn check_http_reachability_reports_error_when_health_is_unhealthy() {
        with_fake_curl_for_urls(200, 404, || {
            with_fake_docker_status("running", || {
                assert!(check_http_reachability(&service()));
            });
        });
    }

    #[test]
    fn check_http_reachability_returns_error_when_service_is_not_running() {
        with_fake_docker_status("exited", || {
            assert!(check_http_reachability(&service()));
        });
    }

    #[test]
    fn check_http_reachability_uses_health_default_when_target_missing_container_name() {
        let target = ServiceConfig {
            container_name: None,
            resolved_container_name: None,
            runtime: Default::default(),
            ..service()
        };

        assert!(check_http_reachability(&target));
    }
}
