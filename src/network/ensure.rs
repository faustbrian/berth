//! Creates instance networks on demand, safely across concurrent processes.

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::subnet::candidate_subnet;

/// Networks already verified by this process.
static ENSURED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

/// Upper bound on subnet probes before giving up.
const MAX_SUBNET_ATTEMPTS: u32 = 256;

/// Ensures the service's instance network exists and belongs to this
/// installation.
///
/// # Errors
///
/// Returns an error when Docker is unreachable, the network name is taken by a
/// network berth did not create, or no free subnet is found.
pub(crate) fn ensure_service_network(service: &ServiceConfig) -> Result<()> {
    let Some(network) = &service.runtime.network else {
        return Ok(());
    };
    if crate::docker::is_dry_run() {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("[dry-run] Ensure network {network}"),
            Persistence::Transient,
        );
        return Ok(());
    }
    let ensured = ENSURED.get_or_init(Default::default);
    if ensured
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .contains(network)
    {
        return Ok(());
    }

    let installation = crate::home::installation_id()?;
    // One allocation at a time: two processes probing the same subnet would
    // otherwise both see it free.
    let _lock = crate::home::HostLock::acquire("network-allocation")?;
    match network_installation_label(network)? {
        Some(owner) if owner == installation => {}
        Some(_) | None if network_exists(network)? => anyhow::bail!(
            "Docker network {network} exists but was not created by this berth installation; \
             remove it or choose another --instance"
        ),
        _ => create_network(service, network, &installation)?,
    }
    ensured
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(network.clone());
    Ok(())
}

fn network_exists(network: &str) -> Result<bool> {
    let output = crate::docker::run_docker_output_owned(
        &[
            "network".to_owned(),
            "inspect".to_owned(),
            network.to_owned(),
        ],
        "failed to inspect Docker network",
    )?;
    Ok(output.status.success())
}

fn network_installation_label(network: &str) -> Result<Option<String>> {
    let output = crate::docker::run_docker_output_owned(
        &[
            "network".to_owned(),
            "inspect".to_owned(),
            "--format".to_owned(),
            "{{index .Labels \"dev.berth.installation\"}}".to_owned(),
            network.to_owned(),
        ],
        "failed to inspect Docker network",
    )?;
    if !output.status.success() {
        return Ok(None);
    }
    let label = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok((!label.is_empty() && label != "<no value>").then_some(label))
}

fn create_network(service: &ServiceConfig, network: &str, installation: &str) -> Result<()> {
    let mut labels = vec![
        "--label".to_owned(),
        "dev.berth.managed=true".to_owned(),
        "--label".to_owned(),
        format!("dev.berth.installation={installation}"),
        "--label".to_owned(),
        "dev.berth.role=network".to_owned(),
    ];
    if let Some(project) = &service.runtime.project {
        labels.extend(["--label".to_owned(), format!("dev.berth.project={project}")]);
    }
    if let Some(instance) = &service.runtime.instance {
        labels.extend([
            "--label".to_owned(),
            format!("dev.berth.instance={instance}"),
        ]);
    }

    let mut last_error = String::new();
    for attempt in 0..MAX_SUBNET_ATTEMPTS {
        let subnet = candidate_subnet(installation, network, attempt);
        let mut args = vec![
            "network".to_owned(),
            "create".to_owned(),
            "--driver".to_owned(),
            "bridge".to_owned(),
            "--subnet".to_owned(),
            subnet.clone(),
        ];
        args.extend(labels.iter().cloned());
        args.push(network.to_owned());
        let output =
            crate::docker::run_docker_output_owned(&args, "failed to create Docker network")
                .context("failed to run docker network create")?;
        if output.status.success() {
            output::event(
                &service.name,
                LogLevel::Info,
                &format!("Created network {network} ({subnet})"),
                Persistence::Persistent,
            );
            return Ok(());
        }
        last_error = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        // Docker: "Pool overlaps with other one on this address space";
        // Podman: "subnet ... is already used on the host or by another config".
        if !is_subnet_conflict(&last_error) {
            anyhow::bail!("failed to create Docker network {network}: {last_error}");
        }
    }
    anyhow::bail!(
        "no free subnet for {network} after {MAX_SUBNET_ATTEMPTS} attempts in 10.128.0.0/9: {last_error}"
    )
}

fn is_subnet_conflict(stderr: &str) -> bool {
    stderr.contains("overlap") || stderr.contains("is already used")
}

#[cfg(test)]
mod tests {
    use super::is_subnet_conflict;

    #[test]
    fn docker_and_podman_subnet_conflicts_are_retried_and_other_errors_are_not() {
        assert!(is_subnet_conflict(
            "Error response from daemon: invalid pool request: Pool overlaps with other one on this address space"
        ));
        assert!(is_subnet_conflict(
            "Error: subnet 10.200.1.0/24 is already used on the host or by another config"
        ));
        assert!(!is_subnet_conflict(
            "Error response from daemon: permission denied"
        ));
        assert!(!is_subnet_conflict(
            "Error: network name berth_a_b already used"
        ));
    }
}
