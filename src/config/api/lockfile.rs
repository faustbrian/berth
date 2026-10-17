//! config api lockfile module.
//!
//! Contains config api lockfile logic used by Berth command workflows.

use anyhow::{Context, Result};
use std::path::PathBuf;

use super::super::{Config, LockedImage, Lockfile};
use super::project::ProjectRootPathOptions;
pub use diff::LockfileDiff;
pub use diff::lockfile_diff;
use digest::resolve_image_digest;

mod diff;
mod digest;

/// Builds image lock for command execution.
pub fn build_image_lock(config: &Config) -> Result<Lockfile> {
    let mut images: Vec<LockedImage> = config
        .service
        .iter()
        .map(|service| {
            Ok(LockedImage {
                service: service.name.clone(),
                image: service.image.clone(),
                resolved: resolve_image_digest(&service.image)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    images.sort_by(|left, right| left.service.cmp(&right.service));
    Ok(Lockfile { version: 1, images })
}

/// Loads lockfile with from persisted or external state.
pub fn load_lockfile_with(options: ProjectRootPathOptions<'_>) -> Result<Lockfile> {
    let path = super::config_io::resolve_lockfile_path(options)?;
    super::config_io::read_yaml_file(&path, "lockfile")
}

/// Saves lockfile with to persisted or external state.
pub fn save_lockfile_with(
    lockfile: &Lockfile,
    options: ProjectRootPathOptions<'_>,
) -> Result<PathBuf> {
    let path = super::config_io::resolve_lockfile_path(options)?;
    super::config_io::write_yaml_file(&path, lockfile, "lockfile")?;
    Ok(path)
}

/// Verifies lockfile with and reports actionable failures.
pub fn verify_lockfile_with(config: &Config, options: ProjectRootPathOptions<'_>) -> Result<()> {
    let expected = build_image_lock(config)?;
    let actual = load_lockfile_with(options)
        .context("failed to load .berth.lock.yaml; run `berth lock images` to generate it")?;
    let diff = lockfile_diff(&expected, &actual);
    if diff.missing.is_empty() && diff.changed.is_empty() && diff.extra.is_empty() {
        return Ok(());
    }

    anyhow::bail!("lockfile is out of sync; run `berth lock images`")
}

#[cfg(test)]
mod tests {
    use super::{
        LockedImage, build_image_lock, load_lockfile_with, save_lockfile_with, verify_lockfile_with,
    };
    use crate::config::{Config, Driver, Kind, Lockfile, ProjectRootPathOptions, ServiceConfig};
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn service(name: &str, image: &str) -> ServiceConfig {
        ServiceConfig {
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
        }
    }

    fn config() -> Config {
        Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: Some("berth".to_owned()),
            domain_strategy: None,
            service: vec![
                service("db", "postgres@sha256:db"),
                service("app", "nginx@sha256:app"),
            ],
            swarm: Vec::new(),
        }
    }

    fn temp_dir() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "berth-lockfile-tests-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        drop(fs::remove_dir_all(&path));
        fs::create_dir_all(&path).expect("create temp project");
        path
    }

    fn config_path(root: &Path) -> std::path::PathBuf {
        root.join(".berth.yaml")
    }

    #[test]
    fn build_image_lock_orders_by_service_name() -> anyhow::Result<()> {
        let cfg = config();
        let lockfile = build_image_lock(&cfg)?;
        assert_eq!(lockfile.images[0].service, "app");
        assert_eq!(lockfile.images.len(), 2);
        assert_eq!(lockfile.images[0].service, "app");
        assert_eq!(lockfile.images[0].image, "nginx@sha256:app");
        assert_eq!(lockfile.images[0].resolved, "nginx@sha256:app");
        assert_eq!(lockfile.images[1].service, "db");
        assert_eq!(lockfile.images[1].image, "postgres@sha256:db");
        assert_eq!(lockfile.images[1].resolved, "postgres@sha256:db");
        Ok(())
    }

    #[test]
    fn load_and_save_lockfile_roundtrip() -> anyhow::Result<()> {
        let root = temp_dir();
        let config_path = config_path(&root);
        let lockfile = Lockfile {
            version: 1,
            images: vec![LockedImage {
                service: "app".to_owned(),
                image: "nginx@sha256:app".to_owned(),
                resolved: "nginx@sha256:app".to_owned(),
            }],
        };
        let options = ProjectRootPathOptions::new(Some(&config_path), None);

        let saved = save_lockfile_with(&lockfile, options)?;
        let loaded = load_lockfile_with(ProjectRootPathOptions::new(Some(&saved), None))?;
        assert_eq!(loaded.version, lockfile.version);
        assert_eq!(loaded.images.len(), 1);
        assert_eq!(loaded.images[0].service, "app");

        Ok(())
    }

    #[test]
    fn verify_lockfile_with_detects_out_of_sync_data() -> anyhow::Result<()> {
        let root = temp_dir();
        let config_path = config_path(&root);
        let cfg = config();
        let expected = build_image_lock(&cfg)?;
        let mut mismatch = expected.clone();
        mismatch.images[0].resolved = "unexpected".to_owned();

        let options = ProjectRootPathOptions::new(Some(&config_path), None);
        save_lockfile_with(&mismatch, options)?;

        let actual =
            verify_lockfile_with(&cfg, ProjectRootPathOptions::new(Some(&config_path), None));
        assert!(actual.is_err());
        Ok(())
    }
}
