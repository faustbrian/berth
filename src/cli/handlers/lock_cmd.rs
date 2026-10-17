//! cli handlers lock cmd module.
//!
//! Contains cli handlers lock cmd logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

use crate::cli::args::LockCommands;
use crate::cli::handlers::log;
use crate::config;

pub(crate) fn handle_lock(
    config_data: &config::Config,
    command: &LockCommands,
    quiet: bool,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<()> {
    match command {
        LockCommands::Images => {
            let lockfile = config::build_image_lock(config_data)?;
            let path = config::save_lockfile_with(
                &lockfile,
                config::ProjectRootPathOptions::new(config_path, project_root),
            )?;
            log::info_if_not_quiet(
                quiet,
                "lock",
                &format!(
                    "Wrote {} with {} image entries",
                    path.display(),
                    lockfile.images.len()
                ),
            );
            Ok(())
        }
        LockCommands::Verify => {
            let expected = config::build_image_lock(config_data)?;
            let actual = config::load_lockfile_with(config::ProjectRootPathOptions::new(
                config_path,
                project_root,
            ))?;
            let diff = config::lockfile_diff(&expected, &actual);

            if diff.missing.is_empty() && diff.changed.is_empty() && diff.extra.is_empty() {
                log::info_if_not_quiet(quiet, "lock", "Lockfile is in sync");
                return Ok(());
            }

            print_diff(&diff);
            anyhow::bail!("lockfile is out of sync; run `berth lock images`")
        }
        LockCommands::Diff => {
            let expected = config::build_image_lock(config_data)?;
            let actual = config::load_lockfile_with(config::ProjectRootPathOptions::new(
                config_path,
                project_root,
            ))
            .unwrap_or_else(|_| config::Lockfile::default());
            let diff = config::lockfile_diff(&expected, &actual);
            print_diff(&diff);
            Ok(())
        }
    }
}

fn print_diff(diff: &config::LockfileDiff) {
    if diff.missing.is_empty() && diff.changed.is_empty() && diff.extra.is_empty() {
        println!("No lockfile changes");
        return;
    }

    for item in &diff.missing {
        println!("+ {} {} -> {}", item.service, item.image, item.resolved);
    }
    for (expected, actual) in &diff.changed {
        println!(
            "~ {} {} -> {} (was {})",
            expected.service, expected.image, expected.resolved, actual.resolved
        );
    }
    for item in &diff.extra {
        println!("- {} {} -> {}", item.service, item.image, item.resolved);
    }
}

#[cfg(test)]
mod tests {
    use crate::cli::args::LockCommands;
    use crate::config::{Config, Driver, Kind, LockedImage, Lockfile, ServiceConfig};
    use crate::docker;
    use std::io::Write;

    use super::handle_lock;

    fn service(name: &str) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind: Kind::App,
            driver: Driver::Frankenphp,
            image: "php:8.4".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 33065,
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
            service: vec![service("app"), {
                let mut redis = service("cache");
                redis.driver = Driver::Redis;
                redis.image = "redis:7".to_owned();
                redis
            }],
            swarm: Vec::new(),
        }
    }

    fn project_root() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "berth-lock-cmd-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::remove_dir_all(&path).ok();
        std::fs::create_dir_all(&path).expect("create project root");
        path
    }

    fn set_lockfile(root: &std::path::Path, entries: Vec<LockedImage>) {
        let lockfile = Lockfile {
            version: 1,
            images: entries,
        };
        let path = root.join(".berth.lock.yaml");
        let content = crate::config::yaml::to_string(&lockfile).expect("serialize lockfile");
        std::fs::write(&path, content).expect("write lockfile");
    }

    fn with_fake_docker<F, T>(test: F) -> T
    where
        F: FnOnce() -> T,
    {
        let bin_dir = std::env::temp_dir().join(format!(
            "berth-lock-cmd-docker-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&bin_dir).expect("create temp dir");
        let binary = bin_dir.join("docker");
        let mut file = std::fs::File::create(&binary).expect("create fake docker");
        writeln!(
            file,
            "#!/bin/sh\n\
if [ \"$1\" = \"image\" ] && [ \"$2\" = \"inspect\" ]; then\n\
  if [ \"$3\" = \"--format\" ]; then\n\
    printf 'sha256:deadbeef'\n\
  fi\n\
  exit 0\n\
elif [ \"$1\" = \"pull\" ]; then\n\
  exit 0\n\
else\n\
  exit 0\n\
fi\n"
        )
        .expect("write fake docker");

        drop(file);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&binary).expect("metadata").permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&binary, perms).expect("chmod");
        }

        let path = binary.to_string_lossy().to_string();
        let result = docker::with_docker_command(&path, || test());
        std::fs::remove_dir_all(&bin_dir).ok();
        result
    }

    #[test]
    fn handle_lock_images_writes_lockfile() {
        let root = project_root();
        let cfg = config();
        let lock_path = root.join(".berth.lock.yaml");

        let cfg_path = root.join("custom.berth.yaml");
        let cfg_path = cfg_path.as_path();
        let result = with_fake_docker(|| {
            handle_lock(
                &cfg,
                &LockCommands::Images,
                false,
                Some(cfg_path),
                Some(&root),
            )
        });
        let result = result.expect("write lockfile");
        assert_eq!(result, ());
        assert!(lock_path.exists());
    }

    #[test]
    fn handle_lock_verify_succeeds_when_in_sync_and_fails_when_out_of_sync() {
        let root = project_root();
        let cfg = config();
        let cfg_path = root.join("custom.berth.yaml");
        let cfg_path = cfg_path.as_path();
        set_lockfile(
            &root,
            vec![
                LockedImage {
                    service: "app".to_owned(),
                    image: "php:8.4".to_owned(),
                    resolved: "sha256:deadbeef".to_owned(),
                },
                LockedImage {
                    service: "cache".to_owned(),
                    image: "redis:7".to_owned(),
                    resolved: "sha256:deadbeef".to_owned(),
                },
            ],
        );
        with_fake_docker(|| {
            handle_lock(
                &cfg,
                &LockCommands::Verify,
                false,
                Some(cfg_path),
                Some(&root),
            )
        })
        .expect("verify in sync");

        set_lockfile(
            &root,
            vec![LockedImage {
                service: "missing".to_owned(),
                image: "x".to_owned(),
                resolved: "y".to_owned(),
            }],
        );
        let error = with_fake_docker(|| {
            handle_lock(
                &cfg,
                &LockCommands::Verify,
                false,
                Some(cfg_path),
                Some(&root),
            )
        })
        .expect_err("expect lockfile out of sync");
        assert!(error.to_string().contains("is out of sync"));
    }

    #[test]
    fn handle_lock_diff_prints_missing_and_extra_entries() {
        let root = project_root();
        let cfg = config();
        let cfg_path = root.join("custom.berth.yaml");
        let cfg_path = cfg_path.as_path();

        set_lockfile(
            &root,
            vec![LockedImage {
                service: "app".to_owned(),
                image: "php:8.4".to_owned(),
                resolved: "old".to_owned(),
            }],
        );
        let result = with_fake_docker(|| {
            handle_lock(
                &cfg,
                &LockCommands::Diff,
                false,
                Some(cfg_path),
                Some(&root),
            )
        });
        assert!(result.is_ok());
    }
}
