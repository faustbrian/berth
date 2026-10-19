//! cli support run doctor repro module.
//!
//! Contains cli support run doctor repro logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

use super::report;
use crate::config;

/// Checks reproducibility and reports actionable failures.
pub(super) fn check_reproducibility(
    config_data: &config::Config,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<bool> {
    let mut has_error = false;

    for service in &config_data.service {
        if service.image.contains("@sha256:") {
            continue;
        }

        has_error = true;
        report::error(&format!(
            "Service '{}' uses non-immutable image '{}'",
            service.name, service.image
        ));
    }

    if let Err(err) = config::verify_lockfile_with(
        config_data,
        config::ProjectRootPathOptions::new(config_path, project_root),
    ) {
        has_error = true;
        report::error(&format!("Lockfile check failed: {err}"));
    }

    if !has_error {
        report::success("Reproducibility checks passed");
    }

    Ok(has_error)
}

#[cfg(test)]
mod tests {
    use super::check_reproducibility;
    use crate::config::{Driver, Kind, LockedImage, ProjectRootPathOptions, ServiceConfig};
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn config_with_service(name: &str, image: &str) -> crate::config::Config {
        crate::config::Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![ServiceConfig {
                name: name.to_owned(),
                kind: Kind::App,
                driver: Driver::Frankenphp,
                image: image.to_owned(),
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
                container_name: Some(format!("{name}-container")),
                resolved_container_name: None,
                runtime: Default::default(),
            }],
            swarm: Vec::new(),
        }
    }

    fn temp_root() -> std::path::PathBuf {
        let root = crate::test_support::temp_root().join(format!(
            "berth-repro-tests-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    fn config_path(root: &Path) -> std::path::PathBuf {
        let path = root.join(".berth.yaml");
        crate::config::project_file::write_test_config_file(
            &path,
            "schema_version = 1\nproject_type = \"project\"\n",
        )
        .expect("write config");
        path
    }

    #[test]
    fn check_reproducibility_fails_for_non_immutable_images() {
        let root = temp_root();
        let path = config_path(&root);
        let config = config_with_service("app", "nginx:1.29");
        let has_error = check_reproducibility(&config, Some(&path), None).expect("repro check");
        assert!(has_error);
    }

    #[test]
    fn check_reproducibility_passes_for_immutable_and_matching_lockfile() -> anyhow::Result<()> {
        let root = temp_root();
        let path = config_path(&root);
        let cfg = config_with_service("app", "nginx@sha256:deadbeef");
        let lockfile = crate::config::Lockfile {
            version: 1,
            images: vec![LockedImage {
                service: "app".to_owned(),
                image: "nginx@sha256:deadbeef".to_owned(),
                resolved: "nginx@sha256:deadbeef".to_owned(),
            }],
        };
        crate::config::save_lockfile_with(
            &lockfile,
            ProjectRootPathOptions::new(Some(&path), None),
        )?;

        let has_error = check_reproducibility(&cfg, Some(&path), None)?;
        assert!(!has_error);
        Ok(())
    }
}
