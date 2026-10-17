//! cli handlers env cmd managed runtime sync module.
//!
//! Contains cli handlers env cmd managed runtime sync logic used by Berth command workflows.

use std::collections::{HashMap, HashSet};

use crate::{cli, config, docker};

pub(super) fn sync_managed_values_from_runtime(
    config: &config::Config,
    selected_names: &HashSet<String>,
    managed_keys: &HashSet<String>,
    managed_values: &mut HashMap<String, String>,
) {
    let app_targets: Vec<&config::ServiceConfig> = if selected_names.is_empty() {
        cli::support::app_services(config)
    } else {
        cli::support::app_services(config)
            .into_iter()
            .filter(|svc| selected_names.contains(&svc.name))
            .collect()
    };

    for target in app_targets {
        let Ok(container_name) = target.container_name() else {
            continue;
        };

        if docker::inspect_status(&container_name).as_deref() != Some("running") {
            continue;
        }

        let Some(runtime_vars) = docker::inspect_env(&container_name) else {
            continue;
        };

        for key in managed_keys {
            if let Some(value) = runtime_vars.get(key) {
                managed_values.insert(key.clone(), value.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::{config, docker};

    fn app_service(name: &str, container_name: &str, runtime_port: u16) -> config::ServiceConfig {
        config::ServiceConfig {
            name: name.to_owned(),
            kind: config::Kind::App,
            driver: config::Driver::Frankenphp,
            image: "service:latest".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: runtime_port,
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
            container_name: Some(container_name.to_owned()),
            resolved_container_name: None,
            runtime: Default::default(),
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
        let bin_dir = std::env::temp_dir().join(format!("berth-runtime-sync-{stamp}"));
        std::fs::create_dir_all(&bin_dir).expect("create fake docker dir");
        let command = bin_dir.join("docker");
        std::fs::write(&command, format!("#!/bin/sh\n{script}")).expect("write fake docker");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&command).expect("metadata").permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&command, perms).expect("chmod");
        }
        let result = docker::with_dry_run_state(false, || {
            docker::with_docker_command(&command.to_string_lossy(), || test())
        });
        std::fs::remove_dir_all(&bin_dir).ok();
        result
    }

    #[test]
    fn sync_managed_values_from_runtime_updates_matching_keys_for_running_apps() {
        let config = config_with(vec![app_service("app", "app-running", 8080)]);
        let selected = HashSet::from(["app".to_owned()]);
        let managed_keys = HashSet::from(["CUSTOM".to_owned()]);
        let mut managed_values = HashMap::from([("CUSTOM".to_owned(), "stale".to_owned())]);

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
                super::sync_managed_values_from_runtime(
                    &config,
                    &selected,
                    &managed_keys,
                    &mut managed_values,
                );
            },
        );

        assert_eq!(
            managed_values.get("CUSTOM"),
            Some(&"from-runtime".to_owned())
        );
    }

    #[test]
    fn sync_managed_values_from_runtime_ignores_stopped_app() {
        let config = config_with(vec![app_service("app", "app-stopped", 8080)]);
        let selected = HashSet::from(["app".to_owned()]);
        let managed_keys = HashSet::from(["CUSTOM".to_owned()]);
        let mut managed_values = HashMap::from([("CUSTOM".to_owned(), "keep".to_owned())]);

        with_fake_docker(
            r#"
if [ "$1" = "inspect" ] && [ "$2" = "--format={{.State.Status}}" ] && [ "$3" = "app-stopped" ]; then
  printf 'created'
elif [ "$1" = "inspect" ] && [ "$2" = "app-stopped" ]; then
  printf 'CUSTOM=ignored\n'
else
  printf ''
fi
if [ "$1" = "inspect" ]; then
  exit 0
fi
exit 1
"#,
            || {
                super::sync_managed_values_from_runtime(
                    &config,
                    &selected,
                    &managed_keys,
                    &mut managed_values,
                );
            },
        );

        assert_eq!(managed_values.get("CUSTOM"), Some(&"keep".to_owned()));
    }
}
