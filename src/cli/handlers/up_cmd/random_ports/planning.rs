//! up random-ports planning helpers.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::cli::args::PortStrategyArg;
use crate::{cli, config, docker};

use super::port_assignment::{
    assign_runtime_port, effective_port_seed, explicit_port_service_names, should_randomize_port,
};

pub(super) struct RuntimePlan {
    pub(super) planned: Vec<(config::ServiceConfig, bool)>,
    pub(super) app_env: HashMap<String, String>,
}

pub(super) struct PlanRuntimeStartupOptions<'a> {
    pub(super) service: Option<&'a str>,
    pub(super) kind: Option<config::Kind>,
    pub(super) profile: Option<&'a str>,
    pub(super) force_random_ports: bool,
    pub(super) port_strategy: PortStrategyArg,
    pub(super) port_seed: Option<&'a str>,
    pub(super) workspace_root: &'a Path,
    pub(super) runtime_env: Option<&'a str>,
    pub(super) config_path: Option<&'a Path>,
    pub(super) project_root: Option<&'a Path>,
    pub(super) project_dependency_env: &'a HashMap<String, String>,
}

pub(super) fn plan_runtime_startup(
    config: &config::Config,
    options: PlanRuntimeStartupOptions<'_>,
) -> Result<RuntimePlan> {
    let (explicit_port_services, explicit_smtp_services) =
        explicit_port_service_names(options.config_path, options.project_root)?;
    let mut runtime_config = config.clone();
    let selected: Vec<config::ServiceConfig> =
        cli::support::select_up_targets(config, options.service, options.kind, options.profile)?
            .into_iter()
            .cloned()
            .collect();
    let mut used_ports = cli::support::collect_service_host_ports(&runtime_config.service);
    let seed = effective_port_seed(
        options.workspace_root,
        options.runtime_env,
        options.port_seed,
    );

    let mut planned = Vec::new();
    for mut runtime in selected {
        let uses_random_port = should_randomize_port(
            &explicit_port_services,
            &runtime.name,
            options.force_random_ports,
        );
        if uses_random_port {
            runtime.port = match running_container_port(&runtime) {
                Some(port) => port,
                None => assign_runtime_port(
                    &runtime,
                    options.port_strategy,
                    &seed,
                    &mut used_ports,
                    "port",
                )?,
            };
        }
        if runtime.driver == config::Driver::Mailhog
            && should_randomize_port(
                &explicit_smtp_services,
                &runtime.name,
                options.force_random_ports,
            )
        {
            runtime.smtp_port = Some(assign_runtime_port(
                &runtime,
                options.port_strategy,
                &seed,
                &mut used_ports,
                "smtp_port",
            )?);
        }
        cli::support::insert_service_host_ports(&mut used_ports, &runtime);
        cli::support::apply_runtime_binding(&mut runtime_config, &runtime)?;
        planned.push((runtime, uses_random_port));
    }

    let app_env = cli::support::runtime_app_env(&runtime_config, options.project_dependency_env);

    Ok(RuntimePlan { planned, app_env })
}

fn running_container_port(service: &config::ServiceConfig) -> Option<u16> {
    let container_name = service.container_name().ok()?;
    if docker::inspect_status(&container_name).as_deref() != Some("running") {
        return None;
    }

    docker::inspect_host_port_binding(&container_name, service.default_port()).map(|(_, port)| port)
}

#[cfg(test)]
mod tests {
    use super::{PlanRuntimeStartupOptions, plan_runtime_startup};
    use crate::cli::args::PortStrategyArg;
    use crate::config::{Config, Driver, Kind, ProjectType, ServiceConfig};
    use std::collections::HashMap;
    use std::fs;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_suffix() -> String {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos()
            .to_string()
    }

    fn app_service() -> ServiceConfig {
        ServiceConfig {
            name: "app".to_owned(),
            kind: Kind::App,
            driver: Driver::Frankenphp,
            image: "dunglas/frankenphp:php8.5".to_owned(),
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
            domain: Some("app.berth".to_owned()),
            domains: None,
            resolved_domain: None,
            container_port: Some(80),
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
        }
    }

    fn db_service(port: u16) -> ServiceConfig {
        ServiceConfig {
            name: "db".to_owned(),
            kind: Kind::Database,
            driver: Driver::Mysql,
            image: "mysql:8.4".to_owned(),
            host: "127.0.0.1".to_owned(),
            port,
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
            container_port: Some(3306),
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
            container_name: Some("acme-db".to_owned()),
            resolved_container_name: Some("acme-db".to_owned()),
        }
    }

    fn config() -> Config {
        Config {
            schema_version: 1,
            project_type: ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![app_service(), db_service(33060)],
            swarm: Vec::new(),
        }
    }

    fn with_fake_docker<T>(script: &str, test: impl FnOnce(std::path::PathBuf) -> T) -> T {
        let root = std::env::temp_dir().join(format!("berth-random-ports-{}", unique_suffix()));
        fs::create_dir_all(&root).expect("create temp root");
        let docker = root.join("docker");
        let mut file = fs::File::create(&docker).expect("create fake docker");
        writeln!(file, "#!/bin/sh\n{script}").expect("write fake docker");
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&docker).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&docker, perms).expect("chmod fake docker");
        }

        let config_path = root.join(".berth.yaml");
        crate::config::project_file::write_test_config_file(&config_path,
            "schema_version = 1\nproject_type = \"project\"\n[[service]]\nname = \"app\"\n[[service]]\nname = \"db\"\n",
        )
        .expect("write config");

        let result =
            crate::docker::with_docker_command(&docker.to_string_lossy(), || test(config_path));
        fs::remove_dir_all(&root).ok();
        result
    }

    #[test]
    fn plan_runtime_startup_reuses_running_container_port_binding() {
        with_fake_docker(
            r#"
if [ "$1" = "inspect" ] && [ "$2" = "--format={{.State.Status}}" ] && [ "$3" = "acme-db" ]; then
  printf 'running'
  exit 0
fi
if [ "$1" = "inspect" ] && [ "$2" = "acme-db" ]; then
  printf '%s' '[{"NetworkSettings":{"Ports":{"3306/tcp":[{"HostIp":"127.0.0.1","HostPort":"42123"}]}}}]'
  exit 0
fi
exit 1
"#,
            |config_path| {
                let workspace_root = config_path.parent().expect("workspace root").to_path_buf();
                let plan = plan_runtime_startup(
                    &config(),
                    PlanRuntimeStartupOptions {
                        service: Some("db"),
                        kind: None,
                        profile: None,
                        force_random_ports: false,
                        port_strategy: PortStrategyArg::Random,
                        port_seed: None,
                        workspace_root: &workspace_root,
                        runtime_env: None,
                        config_path: Some(&config_path),
                        project_root: None,
                        project_dependency_env: &HashMap::new(),
                    },
                )
                .expect("plan runtime startup");

                assert_eq!(plan.planned.len(), 1);
                assert_eq!(plan.planned[0].0.name, "db");
                assert_eq!(plan.planned[0].0.port, 42123);
                assert!(plan.planned[0].1);
            },
        );
    }
}
