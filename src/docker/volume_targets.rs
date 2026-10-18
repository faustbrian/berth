//! The named volumes that hold a service's data.

use anyhow::Result;
use std::collections::BTreeSet;
use std::path::Path;

use crate::config::{Driver, ServiceConfig};

/// Returns the Docker volumes holding the service's data: its default
/// `{container}-data` volume, or its authored named volumes.
///
/// # Errors
///
/// Returns an error when the container name cannot be resolved.
pub(crate) fn collect_named_volume_targets(service: &ServiceConfig) -> Result<Vec<String>> {
    let mut named_volumes = BTreeSet::new();
    // App mounts are host paths relative to the project (`storage:/app/storage`
    // binds `{root}/storage`): berth creates no volume for them, and a volume
    // of that name belongs to someone else.
    if service.kind == crate::config::Kind::App || service.runtime.shared.is_some() {
        return Ok(Vec::new());
    }

    if service.volumes.is_none() && uses_default_named_data_volume(service) {
        named_volumes.insert(format!("{}-data", service.container_name()?));
    }

    if let Some(volumes) = &service.volumes {
        for volume in volumes {
            if let Some(named_volume) = extract_named_volume_source(volume)
                && owned_by_instance(service, &named_volume)
            {
                named_volumes.insert(named_volume);
            }
        }
    }

    Ok(named_volumes.into_iter().collect())
}

/// Whether berth created this named volume for the service's instance: it
/// lives in the instance namespace `berth_{project}_{instance}_`. Services
/// without an instance identity (in-memory configs) own none.
fn owned_by_instance(service: &ServiceConfig, volume: &str) -> bool {
    let (Some(project), Some(instance)) = (&service.runtime.project, &service.runtime.instance)
    else {
        return false;
    };
    // `_` ends the instance ID, which cannot contain one: `agent-1` never
    // claims `agent-1-b`'s volumes.
    volume.starts_with(&format!("berth_{project}_{instance}_"))
}

fn extract_named_volume_source(volume: &str) -> Option<String> {
    let mut parts = volume.splitn(3, ':');
    let source = parts.next()?.trim();
    let _target = parts.next()?;

    if source.is_empty() || is_bind_mount_source(source) {
        return None;
    }

    Some(source.to_owned())
}

fn is_bind_mount_source(source: &str) -> bool {
    source.starts_with('.')
        || source.starts_with('~')
        || source.contains('/')
        || source.contains('\\')
        || Path::new(source).is_absolute()
}

fn uses_default_named_data_volume(service: &ServiceConfig) -> bool {
    matches!(
        service.driver,
        Driver::Mongodb
            | Driver::Postgres
            | Driver::Mysql
            | Driver::Sqlserver
            | Driver::Redis
            | Driver::Valkey
            | Driver::Dragonfly
            | Driver::Garage
            | Driver::Rustfs
            | Driver::Localstack
            | Driver::Opensearch
            | Driver::Elasticsearch
            | Driver::Meilisearch
            | Driver::Typesense
    )
}

/// Removes the service's data volumes (only those berth created). A volume that is already gone is not
/// an error; one still in use is.
///
/// # Errors
///
/// Returns an error when Docker refuses to remove a volume.
pub(crate) fn remove_service_volumes(service: &ServiceConfig) -> Result<()> {
    // A shared-backed service's data is its tenant.
    if service.runtime.shared.is_some() {
        return crate::shared::drop_tenant(service);
    }
    for volume in collect_named_volume_targets(service)? {
        if crate::docker::is_dry_run() {
            crate::docker::print_docker_command(&["volume".to_owned(), "rm".to_owned(), volume]);
            continue;
        }
        if crate::docker::is_foreign_volume(&volume)? {
            crate::output::event(
                &service.name,
                crate::output::LogLevel::Warn,
                &format!("Kept volume {volume}: it belongs to another berth installation"),
                crate::output::Persistence::Persistent,
            );
            continue;
        }
        let output = crate::docker::run_docker_output(
            &["volume", "rm", &volume],
            "failed to remove docker volume",
        )?;
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if output.status.success() || stderr.to_ascii_lowercase().contains("no such volume") {
            continue;
        }
        anyhow::bail!(
            "failed to remove docker volume '{volume}' for service '{}': {stderr}",
            service.name
        );
    }
    Ok(())
}

/// Removes the service's container together with its anonymous volumes. A
/// missing container is not an error.
///
/// # Errors
///
/// Returns an error when Docker refuses to remove the container.
pub(crate) fn remove_container_with_volumes(service: &ServiceConfig) -> Result<()> {
    if service.runtime.shared.is_some() {
        return crate::shared::detach(service);
    }
    let container = service.container_name()?;
    if crate::docker::is_dry_run() {
        crate::docker::print_docker_command(&[
            "rm".to_owned(),
            "-f".to_owned(),
            "-v".to_owned(),
            container,
        ]);
        return Ok(());
    }
    crate::docker::ensure_own_container(&container)?;
    let output = crate::docker::run_docker_output(
        &["rm", "-f", "-v", &container],
        "failed to remove container",
    )?;
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if output.status.success() || stderr.to_ascii_lowercase().contains("no such container") {
        return Ok(());
    }
    anyhow::bail!("failed to remove container {container}: {stderr}")
}
