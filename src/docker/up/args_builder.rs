//! docker up args builder module.
//!
//! Contains docker up args builder logic used by Berth command workflows.

use crate::config::ServiceConfig;

mod entrypoint;
mod env;
mod labels;

use entrypoint::append_entrypoint_args;
use env::append_run_options;
use labels::append_labels;

/// Builds run args for command execution.
pub(super) fn build_run_args(
    service: &ServiceConfig,
    container_name: &str,
) -> anyhow::Result<Vec<String>> {
    let mut args = vec![
        "run".to_owned(),
        "-d".to_owned(),
        "--restart".to_owned(),
        service
            .resolved_restart_policy()
            .as_docker_value()
            .to_owned(),
        "--name".to_owned(),
        container_name.to_owned(),
    ];
    if crate::network::publishes_host_port(service) {
        args.push("-p".to_owned());
        args.push(format!(
            "{}:{}:{}",
            service.host,
            service.port,
            service.default_port()
        ));
    }

    args.extend(crate::network::network_run_args(service, true));
    append_run_options(&mut args, service, container_name);
    append_host_gateway_mapping(&mut args, service);
    append_labels(&mut args, service, container_name)?;
    args.push(service.image.clone());
    append_entrypoint_args(&mut args, service);
    Ok(args)
}

fn append_host_gateway_mapping(args: &mut Vec<String>, service: &ServiceConfig) {
    let host_gateway_alias = crate::docker::host_gateway_alias();
    let env_requests_gateway = service.env.as_ref().is_some_and(|values| {
        values
            .values()
            .any(|value| value.contains(host_gateway_alias))
    });
    if service.uses_host_gateway_alias() || env_requests_gateway {
        if let Some(mapping) = crate::docker::host_gateway_mapping() {
            args.push("--add-host".to_owned());
            args.push(mapping.to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::build_run_args;
    use crate::config::{Driver, Kind, ServiceConfig};
    use std::collections::HashMap;

    fn service() -> ServiceConfig {
        ServiceConfig {
            name: "db".to_owned(),
            kind: Kind::Database,
            driver: Driver::Mysql,
            image: "mysql:8.1".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 3306,
            database: Some("laravel".to_owned()),
            username: Some("laravel".to_owned()),
            password: Some("laravel".to_owned()),
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
            container_name: Some("acme-db".to_owned()),
            resolved_container_name: Some("acme-db".to_owned()),
            runtime: Default::default(),
        }
    }

    #[test]
    fn instance_services_publish_a_host_port_only_when_asked() {
        let mut service = service();
        service.runtime.network = Some("berth_acme_a-1".to_owned());
        let unpublished = build_run_args(&service, "acme-db").expect("run args");
        assert!(
            !unpublished.iter().any(|arg| arg == "-p"),
            "{unpublished:?}"
        );

        service.runtime.publish_host_port = true;
        let published = build_run_args(&service, "acme-db").expect("run args");
        let at = published.iter().position(|arg| arg == "-p").expect("-p");
        assert_eq!(
            published[at + 1],
            format!("127.0.0.1:{}:{}", service.port, service.default_port())
        );

        // Without an instance network the host port is the only way in.
        let mut bare = super::tests::service();
        bare.runtime.network = None;
        assert!(
            build_run_args(&bare, "acme-db")
                .expect("run args")
                .iter()
                .any(|arg| arg == "-p")
        );
    }

    #[test]
    fn adds_host_gateway_mapping_for_loopback_services() {
        crate::docker::with_container_engine(crate::config::ContainerEngine::Docker, || {
            let args = build_run_args(&service(), "acme-db").expect("run args");
            let rendered = args.join(" ");

            assert!(rendered.contains("--add-host host.docker.internal:host-gateway"));
        });
    }

    #[test]
    fn includes_default_restart_policy() {
        let args = build_run_args(&service(), "acme-db").expect("run args");
        let rendered = args.join(" ");

        assert!(rendered.contains("--restart unless-stopped"));
    }

    #[test]
    fn includes_explicit_restart_policy_override() {
        let mut service = service();
        service.restart = Some(crate::config::RestartPolicy::No);

        let args = build_run_args(&service, "acme-db").expect("run args");
        let rendered = args.join(" ");

        assert!(rendered.contains("--restart no"));
    }

    #[test]
    fn adds_host_gateway_mapping_when_custom_env_uses_alias() {
        crate::docker::with_container_engine(crate::config::ContainerEngine::Docker, || {
            let mut with_env = service();
            with_env.host = "10.0.0.10".to_owned();
            with_env.env = Some(HashMap::from([(
                "UPSTREAM".to_owned(),
                "http://host.docker.internal:3306".to_owned(),
            )]));

            let args = build_run_args(&with_env, "acme-db").expect("run args");
            let rendered = args.join(" ");

            assert!(rendered.contains("--add-host host.docker.internal:host-gateway"));
        });
    }

    #[test]
    fn podman_does_not_force_add_host_gateway_mapping() {
        crate::docker::with_container_engine(crate::config::ContainerEngine::Podman, || {
            let args = build_run_args(&service(), "acme-db").expect("run args");
            let rendered = args.join(" ");
            assert!(!rendered.contains("--add-host"));
        });
    }

    #[test]
    fn migrated_8x_postgres_preset_mounts_the_postgres_18_data_directory() {
        let dir =
            std::env::temp_dir().join(format!("berth-migrated-postgres-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(dir.join("composer.json"), "{\"type\":\"project\"}").expect("composer");
        let converted = crate::config::project_file::convert_legacy_config(
            "schema_version: 8\nproject: bill\nservices:\n  db:\n    preset: postgres\n",
            &dir.join(".stackctl.yaml"),
        )
        .expect("convert 8.x file");
        let config_path = dir.join(".berth.yaml");
        std::fs::write(&config_path, converted.content).expect("write berth 9 file");

        let config = crate::config::load_config_with(crate::config::LoadConfigPathOptions::new(
            Some(&config_path),
            None,
        ))
        .expect("load migrated config");
        let db = config
            .service
            .iter()
            .find(|service| service.name == "db")
            .expect("db service");
        let args = build_run_args(db, "bill-db").expect("run args");

        assert!(
            args.iter()
                .any(|arg| arg == "bill-db-data:/var/lib/postgresql"),
            "postgres 18 must not mount /var/lib/postgresql/data: {args:?}"
        );
    }
}
