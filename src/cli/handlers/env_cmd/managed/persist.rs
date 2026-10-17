//! cli handlers env cmd managed persist module.
//!
//! Contains cli handlers env cmd managed persist logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

use crate::cli::handlers::log;
use crate::{config, docker};

pub(super) fn collect_persist_targets(
    selected: &[&config::ServiceConfig],
) -> Vec<(String, String, u16)> {
    selected
        .iter()
        .filter_map(|svc| {
            svc.container_name().ok().map(|container_name| {
                (
                    svc.name.clone(),
                    container_name,
                    svc.resolved_container_port(),
                )
            })
        })
        .collect()
}

pub(super) fn persist_runtime_host_ports(
    config: &mut config::Config,
    persist_targets: &[(String, String, u16)],
    quiet: bool,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<()> {
    let mut changed_services = Vec::new();
    for (service_name, container_name, container_port) in persist_targets {
        if docker::inspect_status(container_name).as_deref() != Some("running") {
            continue;
        }

        let Some((host, port)) = docker::inspect_host_port_binding(container_name, *container_port)
        else {
            continue;
        };

        if config::update_service_host_port(config, service_name, &host, port)? {
            changed_services.push(service_name.clone());
        }
    }

    if !changed_services.is_empty() {
        let path = config::save_config_with(
            config,
            config::SaveConfigPathOptions::new(config_path, project_root),
        )?;
        log::success_if_not_quiet(
            quiet,
            "env",
            &format!(
                "Persisted runtime host/port to {} for: {}",
                path.display(),
                changed_services.join(", ")
            ),
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::{config, docker};

    fn service(
        name: &str,
        driver: config::Driver,
        container_name: Option<&str>,
        port: u16,
    ) -> config::ServiceConfig {
        config::ServiceConfig {
            name: name.to_owned(),
            kind: config::Kind::Database,
            driver,
            image: "service:latest".to_owned(),
            host: "127.0.0.1".to_owned(),
            port,
            database: Some("app".to_owned()),
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
            container_name: container_name.map(ToOwned::to_owned),
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
        let bin_dir = std::env::temp_dir().join(format!("berth-persist-docker-{stamp}"));
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
        fs::remove_dir_all(&bin_dir).ok();

        match result {
            Ok(result) => result,
            Err(error) => std::panic::resume_unwind(error),
        }
    }

    #[test]
    fn collect_persist_targets_skips_services_without_container_name() {
        let services = vec![
            service("app", config::Driver::Mysql, Some("app-1"), 3306),
            service("cache", config::Driver::Redis, None, 6379),
        ];
        let selected = vec![&services[0], &services[1]];
        let targets = super::collect_persist_targets(&selected);
        assert_eq!(targets, vec![("app".to_owned(), "app-1".to_owned(), 3306)]);
    }

    #[test]
    fn persist_runtime_host_ports_updates_matching_service_host_and_port() {
        let mut config = config_with(vec![service(
            "db",
            config::Driver::Mysql,
            Some("db-running"),
            3306,
        )]);
        let targets = vec![("db".to_owned(), "db-running".to_owned(), 3306)];
        let config_path = std::env::temp_dir()
            .join(format!(
                "berth-persist-runtime-{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("time")
                    .as_nanos()
            ))
            .join(".berth.toml");
        fs::create_dir_all(config_path.parent().expect("config dir")).expect("create config dir");
        fs::write(
            &config_path,
            "schema_version = 1\nproject_type = \"project\"\n",
        )
        .expect("seed config file");

        with_fake_docker(
            r#"
if [ "$1" = "inspect" ] && [ "$2" = "--format={{.State.Status}}" ] && [ "$3" = "db-running" ]; then
    printf 'running'
elif [ "$1" = "inspect" ] && [ "$2" = "db-running" ]; then
  printf '[{"NetworkSettings":{"Ports":{"3306/tcp":[{"HostIp":"127.0.0.2","HostPort":"5432"}]}}}]'
else
    printf ''
fi
if [ "$1" = "inspect" ]; then
  exit 0
fi
printf ''
exit 1
"#,
            || {
                super::persist_runtime_host_ports(
                    &mut config,
                    &targets,
                    true,
                    Some(&config_path),
                    None,
                )
                .expect("persist host ports");
            },
        );

        assert_eq!(config.service[0].host, "127.0.0.2");
        assert_eq!(config.service[0].port, 5432);
    }

    #[test]
    fn persist_runtime_host_ports_skips_services_not_running() {
        let mut config = config_with(vec![service(
            "db",
            config::Driver::Mysql,
            Some("db-created"),
            3306,
        )]);
        let targets = vec![("db".to_owned(), "db-created".to_owned(), 3306)];

        with_fake_docker(
            r#"
if [ "$1" = "inspect" ] && [ "$2" = "--format={{.State.Status}}" ] && [ "$3" = "db-created" ]; then
    printf 'created'
    exit 0
fi
printf ''
exit 0
"#,
            || {
                super::persist_runtime_host_ports(
                    &mut config,
                    &targets,
                    false,
                    Some(std::path::Path::new("/tmp/unused.toml")),
                    None,
                )
                .expect("persist skip non-running");
            },
        );

        assert_eq!(config.service[0].host, "127.0.0.1");
        assert_eq!(config.service[0].port, 3306);
    }

    #[test]
    fn persist_runtime_host_ports_skips_missing_host_port_binding() {
        let mut config = config_with(vec![service(
            "db",
            config::Driver::Mysql,
            Some("db-running"),
            3306,
        )]);
        let targets = vec![("db".to_owned(), "db-running".to_owned(), 3306)];

        with_fake_docker(
            r#"
if [ "$1" = "inspect" ] && [ "$2" = "--format={{.State.Status}}" ] && [ "$3" = "db-running" ]; then
    printf 'running'
elif [ "$1" = "inspect" ] && [ "$2" = "db-running" ]; then
    printf '[{"NetworkSettings":{"Ports":{}}}]'
else
    printf ''
fi
if [ "$1" = "inspect" ]; then
  exit 0
fi
printf ''
exit 1
"#,
            || {
                super::persist_runtime_host_ports(
                    &mut config,
                    &targets,
                    true,
                    Some(std::path::Path::new("/tmp/unused.toml")),
                    None,
                )
                .expect("persist without binding");
            },
        );

        assert_eq!(config.service[0].host, "127.0.0.1");
        assert_eq!(config.service[0].port, 3306);
    }
}
