//! cli handlers env cmd managed module.
//!
//! Contains cli handlers env cmd managed logic used by Berth command workflows.

use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;

use crate::cli::handlers::log;
use crate::{cli, config, env};

mod persist;
mod runtime_sync;

use persist::{collect_persist_targets, persist_runtime_host_ports};
use runtime_sync::sync_managed_values_from_runtime;

pub(super) struct ManagedEnvUpdateOptions<'a> {
    pub(super) service: Option<&'a str>,
    pub(super) kind: Option<config::Kind>,
    pub(super) env_path: &'a Path,
    pub(super) sync: bool,
    pub(super) purge: bool,
    pub(super) persist_runtime: bool,
    pub(super) create_missing: bool,
    pub(super) quiet: bool,
    pub(super) config_path: Option<&'a Path>,
    pub(super) project_root: Option<&'a Path>,
}

pub(super) fn handle_managed_env_update(
    config: &mut config::Config,
    options: ManagedEnvUpdateOptions<'_>,
) -> Result<()> {
    let selected = cli::support::selected_services(config, options.service, options.kind, None)?;
    let mut selected_names = HashSet::new();
    for svc in &selected {
        selected_names.insert(svc.name.clone());
    }
    let persist_targets = collect_persist_targets(&selected);

    let mut managed_values = env::managed_app_env(config);
    let managed_keys: HashSet<String> = managed_values.keys().cloned().collect();

    if options.sync {
        sync_managed_values_from_runtime(
            config,
            &selected_names,
            &managed_keys,
            &mut managed_values,
        );
    }

    env::write_env_values_with_purge(
        options.env_path,
        &managed_values,
        options.create_missing,
        &managed_keys,
        options.purge,
    )?;

    if options.persist_runtime {
        persist_runtime_host_ports(
            config,
            &persist_targets,
            options.quiet,
            options.config_path,
            options.project_root,
        )?;
    }

    log::success_if_not_quiet(
        options.quiet,
        "env",
        &format!(
            "Updated {} with managed app env{}{}{}",
            options.env_path.display(),
            if options.sync {
                " (synced from runtime where available)"
            } else {
                ""
            },
            if options.purge {
                " and purged stale managed keys"
            } else {
                ""
            },
            if options.persist_runtime {
                " and persisted runtime host/port to .berth.toml"
            } else {
                ""
            }
        ),
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::{config, docker};

    fn app_service(
        name: &str,
        container_name: &str,
        env_value: (&str, &str),
    ) -> config::ServiceConfig {
        let mut env = HashMap::new();
        env.insert(env_value.0.to_owned(), env_value.1.to_owned());

        config::ServiceConfig {
            name: name.to_owned(),
            kind: config::Kind::App,
            driver: config::Driver::Frankenphp,
            image: "dunglas/frankenphp:php8.5".to_owned(),
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
            env: Some(env),
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
        }
    }

    fn config_with(services: Vec<config::ServiceConfig>) -> config::Config {
        config::Config {
            schema_version: 1,
            project_type: config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: services,
            swarm: Vec::new(),
        }
    }

    fn with_fake_docker<F, T>(script: &str, test: F) -> T
    where
        F: FnOnce() -> T,
    {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let bin_dir = std::env::temp_dir().join(format!("berth-managed-update-{stamp}"));
        fs::create_dir_all(&bin_dir).expect("create fake docker dir");
        let command = bin_dir.join("docker");
        fs::write(&command, format!("#!/bin/sh\n{script}")).expect("write fake docker");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&command).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&command, perms).expect("chmod");
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            docker::with_dry_run_state(false, || {
                docker::with_docker_command(&command.to_string_lossy(), || test())
            })
        }));
        fs::remove_dir_all(bin_dir).ok();
        match result {
            Ok(result) => result,
            Err(error) => std::panic::resume_unwind(error),
        }
    }

    #[test]
    fn handle_managed_env_update_writes_expected_values_without_runtime_sync() -> anyhow::Result<()>
    {
        let mut config = config_with(vec![app_service("app", "app-static", ("CUSTOM", "base"))]);
        let env_path = std::env::temp_dir().join(format!(
            "berth-managed-update-base-{}",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        fs::write(&env_path, "CUSTOM=\"base\"\n")?;

        docker::with_dry_run_lock(|| {
            super::handle_managed_env_update(
                &mut config,
                super::ManagedEnvUpdateOptions {
                    service: Some("app"),
                    kind: None,
                    env_path: &env_path,
                    sync: false,
                    purge: false,
                    persist_runtime: false,
                    create_missing: true,
                    quiet: true,
                    config_path: None,
                    project_root: None,
                },
            )
            .expect("update managed env");
        });

        let contents = fs::read_to_string(&env_path)?;
        assert!(contents.contains("CUSTOM=\"base\""));
        Ok(())
    }

    #[test]
    fn handle_managed_env_update_overrides_explicit_values_from_runtime() -> anyhow::Result<()> {
        let mut config = config_with(vec![app_service("app", "app-running", ("CUSTOM", "base"))]);
        let env_path = std::env::temp_dir().join(format!(
            "berth-managed-update-runtime-{}",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        fs::write(&env_path, "CUSTOM=\"base\"\n")?;

        with_fake_docker(
            r#"
if [ "$1" = "inspect" ] && [ "$2" = "--format={{.State.Status}}" ] && [ "$3" = "app-running" ]; then
  printf 'running'
elif [ "$1" = "inspect" ] && [ "$2" = "--format={{range .Config.Env}}{{println .}}{{end}}" ] && [ "$3" = "app-running" ]; then
  printf 'CUSTOM=from-runtime\n'
else
  printf ''
fi
if [ "$1" = "inspect" ]; then
  exit 0
fi
exit 1
"#,
            || {
                super::handle_managed_env_update(
                    &mut config,
                    super::ManagedEnvUpdateOptions {
                        service: Some("app"),
                        kind: None,
                        env_path: &env_path,
                        sync: true,
                        purge: false,
                        persist_runtime: false,
                        create_missing: true,
                        quiet: true,
                        config_path: None,
                        project_root: None,
                    },
                )
                .expect("runtime sync");
            },
        );

        let contents = fs::read_to_string(&env_path)?;
        assert!(contents.contains("CUSTOM=\"from-runtime\""));
        Ok(())
    }

    #[test]
    fn handle_managed_env_update_persists_runtime_host_port() -> anyhow::Result<()> {
        let mut config = config_with(vec![app_service("app", "app-running", ("CUSTOM", "base"))]);
        let env_path = std::env::temp_dir().join(format!(
            "berth-managed-update-port-{}",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        fs::write(&env_path, "CUSTOM=\"base\"\n")?;
        let root = std::env::temp_dir().join(format!(
            "berth-managed-update-port-root-{}",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        fs::create_dir_all(&root).expect("create config root");
        let config_path = root.join(".berth.toml");
        fs::write(
            &config_path,
            "schema_version = 1\nproject_type = \"project\"\n",
        )
        .expect("seed config file");

        with_fake_docker(
            r#"
if [ "$1" = "inspect" ] && [ "$2" = "--format={{.State.Status}}" ] && [ "$3" = "app-running" ]; then
  printf 'running'
elif [ "$1" = "inspect" ] && [ "$2" = "app-running" ]; then
  printf '[{"NetworkSettings":{"Ports":{"80/tcp":[{"HostIp":"127.0.0.2","HostPort":"5432"}]}}}]'
else
  printf ''
fi
if [ "$1" = "inspect" ]; then
  exit 0
fi
exit 1
"#,
            || {
                super::handle_managed_env_update(
                    &mut config,
                    super::ManagedEnvUpdateOptions {
                        service: Some("app"),
                        kind: None,
                        env_path: &env_path,
                        sync: false,
                        purge: false,
                        persist_runtime: true,
                        create_missing: true,
                        quiet: true,
                        config_path: Some(&config_path),
                        project_root: None,
                    },
                )
                .expect("persist runtime port");
            },
        );

        assert_eq!(config.service[0].host, "127.0.0.2");
        assert_eq!(config.service[0].port, 5432);
        let contents = fs::read_to_string(&env_path)?;
        assert!(contents.contains("CUSTOM=\"base\""));
        Ok(())
    }
}
