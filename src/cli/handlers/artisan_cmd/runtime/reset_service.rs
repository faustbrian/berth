//! Test-runtime reset helpers for artisan test workflows.

use anyhow::Result;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

mod docker_cmd;
use crate::docker::collect_named_volume_targets;
use docker_cmd::{remove_named_volume_output, try_remove_container_with_volumes};

/// Resets a service runtime so test runs start from a clean container state.
pub(super) fn reset_service_runtime(service: &ServiceConfig) -> Result<()> {
    // A shared-backed service's run state is its tenant: dropping it gives
    // the next run a fresh database (recreated when the run starts).
    if service.runtime.shared.is_some() {
        crate::shared::detach(service)?;
        return crate::shared::drop_tenant(service);
    }
    let container_name = service.container_name()?;
    let named_volumes = collect_named_volume_targets(service)?;

    if crate::docker::is_dry_run() {
        crate::docker::print_docker_command(&[
            "rm".to_owned(),
            "-f".to_owned(),
            "-v".to_owned(),
            container_name.clone(),
        ]);
        for volume in named_volumes {
            crate::docker::print_docker_command(&["volume".to_owned(), "rm".to_owned(), volume]);
        }
        return Ok(());
    }

    try_remove_container_with_volumes(&container_name);

    for volume in named_volumes {
        remove_named_volume(service, &volume)?;
    }

    Ok(())
}

fn remove_named_volume(service: &ServiceConfig, volume_name: &str) -> Result<()> {
    let output = remove_named_volume_output(volume_name)?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.to_ascii_lowercase().contains("no such volume") {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("Skipped removing volume {volume_name} because it was not found"),
            Persistence::Persistent,
        );
        return Ok(());
    }

    anyhow::bail!(
        "failed to remove docker volume '{volume_name}' for service '{}': {stderr}",
        service.name
    );
}

#[cfg(test)]
mod tests {
    use crate::config::{Driver, Kind, ServiceConfig};
    use crate::docker::collect_named_volume_targets;

    #[test]
    fn includes_default_data_volume_for_stateful_service_without_explicit_volumes() {
        let service = service("db", Driver::Mysql);

        let volumes = collect_named_volume_targets(&service).expect("collect volumes");
        assert_eq!(volumes, vec!["acme-db-testing-data".to_owned()]);
    }

    #[test]
    fn includes_only_named_explicit_volumes_and_skips_bind_mounts() {
        let mut service = service("db", Driver::Mysql);
        service.runtime.project = Some("shop".to_owned());
        service.runtime.instance = Some("a-1".to_owned());
        service.volumes = Some(vec![
            "berth_shop_a-1_dbdata:/var/lib/mysql".to_owned(),
            "berth_shop_a-1_cache:/data:rw".to_owned(),
            "someone-elses:/data".to_owned(),
            "./src:/app".to_owned(),
            "/tmp/cache:/cache".to_owned(),
            "/cache".to_owned(),
        ]);

        let volumes = collect_named_volume_targets(&service).expect("collect volumes");
        assert_eq!(
            volumes,
            vec![
                "berth_shop_a-1_cache".to_owned(),
                "berth_shop_a-1_dbdata".to_owned()
            ],
            "only volumes in the instance namespace, never binds or foreign volumes"
        );
    }

    fn service(name: &str, driver: Driver) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind: Kind::Database,
            driver,
            image: "mysql:8.1".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 3306,
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
            container_name: Some(format!("acme-{name}")),
            resolved_container_name: Some(format!("acme-{name}-testing")),
            runtime: Default::default(),
        }
    }
}
