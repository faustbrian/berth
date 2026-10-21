//! doctor app php-extension checks.

use crate::cli::support::run_doctor::report;
use crate::{config, serve};

/// Checks php extensions and reports actionable failures.
pub(in crate::cli::support::run_doctor::app) fn check_php_extensions(
    target: &config::ServiceConfig,
) -> bool {
    match serve::verify_php_extensions(target) {
        Ok(Some(check)) if check.missing.is_empty() => {
            report::success(&format!(
                "App service '{}' extensions available in {}",
                check.target, check.image
            ));
            false
        }
        Ok(Some(check)) => {
            report::error(&format!(
                "App service '{}' missing PHP extensions in {}: {}",
                check.target,
                check.image,
                check.missing.join(", ")
            ));
            true
        }
        Ok(None) => false,
        Err(err) => {
            report::error(&format!(
                "App service '{}' extension verification failed: {}",
                target.name, err
            ));
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{config::Driver, config::Kind, config::ServiceConfig, docker};
    use std::fs;
    use std::io::Write;

    use super::check_php_extensions;

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

    #[test]
    fn check_php_extensions_without_configured_extensions() {
        assert!(!check_php_extensions(&service()));
    }

    #[test]
    fn check_php_extensions_is_clean_with_dry_run() {
        let mut target = service();
        target.php_extensions = Some(vec!["pdo".to_owned(), "curl".to_owned()]);
        docker::with_dry_run_state(true, || assert!(!check_php_extensions(&target)));
    }

    #[test]
    fn check_php_extensions_returns_error_when_module_inspection_fails() {
        let bin_dir = crate::test_support::temp_root().join(format!(
            "berth-php-extensions-fail-{}",
            crate::test_support::unique_stamp()
        ));
        fs::create_dir_all(&bin_dir).expect("temp dir");
        let binary = bin_dir.join("docker");
        let mut file = fs::File::create(&binary).expect("create fake docker");
        writeln!(file, "#!/bin/sh\nexit 1").expect("write script");
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&binary).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&binary, perms).expect("chmod");
        }

        let mut target = service();
        target.php_extensions = Some(vec!["pdo".to_owned()]);
        let result = docker::with_dry_run_state(false, || {
            docker::with_docker_command(&binary.to_string_lossy(), || check_php_extensions(&target))
        });
        assert!(result);

        fs::remove_dir_all(&bin_dir).ok();
    }
}
