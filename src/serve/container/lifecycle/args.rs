//! `docker run` argument assembly for serve containers.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::config::ServiceConfig;

mod docker_run_args;
mod env_vars;
use super::super::super::mailhog_smtp_port;
use super::super::run_config::{resolve_volume_mapping, resolved_run_command};
use docker_run_args::{
    append_runtime_image_and_command, append_smtp_port_mapping, append_volume_args,
    build_base_run_args,
};
use env_vars::{append_env_args, append_host_gateway_mapping};

/// Builds the full `docker run` argument list for a serve target.
///
/// Injected env values are added before service-defined env values so explicit
/// service env can override inferred defaults, except protected URL vars where
/// Berth inferred HTTPS should not be downgraded.
pub(super) fn build_run_args(
    target: &ServiceConfig,
    runtime_image: &str,
    project_root: &Path,
    injected_env: &HashMap<String, String>,
    inject_server_name: bool,
) -> Result<Vec<String>> {
    let mut run_args = build_base_run_args(target)?;
    if let Some(smtp_port) = mailhog_smtp_port(target) {
        append_smtp_port_mapping(&mut run_args, &target.host, smtp_port);
    }
    append_host_gateway_mapping(&mut run_args, target, injected_env);

    append_volume_args(&mut run_args, target, project_root, resolve_volume_mapping)?;

    append_env_args(&mut run_args, target, injected_env, inject_server_name);

    append_runtime_image_and_command(&mut run_args, runtime_image, target, resolved_run_command);

    Ok(run_args)
}

#[cfg(test)]
mod tests {
    use super::build_run_args;
    use crate::config::{Driver, Kind, ServiceConfig};
    use std::collections::HashMap;
    use std::path::Path;

    fn app_service() -> ServiceConfig {
        ServiceConfig {
            name: "app".to_owned(),
            kind: Kind::App,
            driver: Driver::Frankenphp,
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
            domain: Some("acme-api.berth".to_owned()),
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
            container_name: Some("acme-api-app".to_owned()),
            resolved_container_name: Some("acme-api-app".to_owned()),
            runtime: Default::default(),
        }
    }

    #[test]
    fn blocks_http_override_for_inferred_https_app_urls() {
        let mut service = app_service();
        service.env = Some(HashMap::from([
            ("APP_URL".to_owned(), "http://localhost".to_owned()),
            ("ASSET_URL".to_owned(), "http://localhost".to_owned()),
        ]));
        let injected = HashMap::from([
            ("APP_URL".to_owned(), "https://acme-api.berth".to_owned()),
            ("ASSET_URL".to_owned(), "https://acme-api.berth".to_owned()),
        ]);

        let args = build_run_args(&service, "runtime-image", Path::new("."), &injected, false)
            .expect("run args");
        let app_https_count = args
            .iter()
            .filter(|v| *v == "APP_URL=https://acme-api.berth")
            .count();
        let app_http_count = args
            .iter()
            .filter(|v| *v == "APP_URL=http://localhost")
            .count();
        let asset_https_count = args
            .iter()
            .filter(|v| *v == "ASSET_URL=https://acme-api.berth")
            .count();
        let asset_http_count = args
            .iter()
            .filter(|v| *v == "ASSET_URL=http://localhost")
            .count();

        assert_eq!(app_https_count, 1);
        assert_eq!(app_http_count, 0);
        assert_eq!(asset_https_count, 1);
        assert_eq!(asset_http_count, 0);
    }

    #[test]
    fn allows_https_override_for_inferred_https_app_urls() {
        let mut service = app_service();
        service.env = Some(HashMap::from([(
            "APP_URL".to_owned(),
            "https://custom.example".to_owned(),
        )]));
        let injected = HashMap::from([("APP_URL".to_owned(), "https://acme-api.berth".to_owned())]);

        let args = build_run_args(&service, "runtime-image", Path::new("."), &injected, false)
            .expect("run args");
        let override_count = args
            .iter()
            .filter(|v| *v == "APP_URL=https://custom.example")
            .count();

        assert_eq!(override_count, 1);
    }

    #[test]
    fn app_containers_carry_instance_ownership_labels() {
        let mut service = app_service();
        service.runtime.project = Some("acme-api".to_owned());
        service.runtime.instance = Some("agent-1-97fe08".to_owned());

        let args = build_run_args(
            &service,
            "runtime-image",
            Path::new("."),
            &HashMap::new(),
            false,
        )
        .expect("run args");
        let rendered = args.join(" ");

        assert!(
            rendered.contains("--label dev.berth.managed=true"),
            "{rendered}"
        );
        assert!(
            rendered.contains("--label dev.berth.instance=agent-1-97fe08"),
            "{rendered}"
        );
        assert!(
            rendered.contains("--label dev.berth.project=acme-api"),
            "{rendered}"
        );
    }

    #[test]
    fn adds_host_gateway_mapping_when_injected_env_uses_alias() {
        let mut service = app_service();
        service.host = "10.0.0.5".to_owned();
        let injected = HashMap::from([("DB_HOST".to_owned(), "host.docker.internal".to_owned())]);

        let args = build_run_args(&service, "runtime-image", Path::new("."), &injected, false)
            .expect("run args");
        let rendered = args.join(" ");

        assert!(rendered.contains("--add-host host.docker.internal:host-gateway"));
    }

    #[test]
    fn podman_skips_host_gateway_mapping_when_loopback_host_used() {
        crate::docker::with_container_engine(crate::config::ContainerEngine::Podman, || {
            let injected = HashMap::new();
            let args = build_run_args(
                &app_service(),
                "runtime-image",
                Path::new("."),
                &injected,
                false,
            )
            .expect("run args");
            let rendered = args.join(" ");
            assert!(!rendered.contains("--add-host"));
        });
    }
}
