//! Removes instance networks that no container uses any more.

use anyhow::Result;
use std::collections::BTreeSet;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

/// Removes each distinct instance network of `services` that this
/// installation created and that no container, running or stopped, is
/// attached to. A stopped container keeps its network's ID; removing the
/// network would make `docker start` fail for it later.
///
/// # Errors
///
/// Returns an error when Docker cannot list containers or remove a network
/// for a reason other than it being gone or in use.
pub(crate) fn remove_unused_networks<'a, I>(services: I) -> Result<()>
where
    I: IntoIterator<Item = &'a ServiceConfig>,
{
    // Each instance network and the swarm endpoint network beside it (L205),
    // which goes once its source app and consumers are gone.
    let names: Vec<String> = services
        .into_iter()
        .filter_map(|service| {
            let network = service.runtime.network.clone()?;
            Some([crate::swarm::endpoint_network(service), Some(network)])
        })
        .flatten()
        .flatten()
        .collect();
    let networks: BTreeSet<&str> = names.iter().map(String::as_str).collect();
    if networks.is_empty() {
        return Ok(());
    }
    if crate::docker::is_dry_run() {
        for network in networks {
            output::event(
                "network",
                LogLevel::Info,
                &format!("[dry-run] Remove network {network} if no container uses it"),
                Persistence::Transient,
            );
        }
        return Ok(());
    }
    let installation = crate::home::installation_id()?;
    let gateway = crate::gateway::gateway_name()?;
    // Serialized with creation so two processes never allocate and remove the
    // same network at once. A concurrent `up` of the same instance can still
    // lose its network between ensuring it and running its container; that
    // `up` then fails with Docker's "network not found" and can be rerun.
    let _lock = crate::home::HostLock::acquire("network-allocation")?;
    for network in networks {
        if network_owner(network)?.as_deref() != Some(installation.as_str()) {
            continue;
        }
        // The gateway is attached to every routed network; it must not keep
        // an otherwise unused network alive, and is detached only once
        // nothing else uses the network.
        if has_containers(network, &gateway)? {
            continue;
        }
        // A network the gateway is attached to is detached and removed under
        // the gateway lock, so a concurrent publish cannot re-attach it in
        // between (lock order: network, gateway). Route-free networks, such as
        // every test lease's, never wait for the gateway lock.
        let _gateway_lock = if crate::gateway::is_attached(network)? {
            let lock = crate::home::HostLock::acquire("gateway")?;
            if has_containers(network, &gateway)? {
                continue;
            }
            crate::gateway::detach_network(network)?;
            Some(lock)
        } else {
            None
        };
        let removed = crate::docker::run_docker_output_owned(
            &["network".to_owned(), "rm".to_owned(), network.to_owned()],
            "failed to remove Docker network",
        )?;
        let stderr = String::from_utf8_lossy(&removed.stderr);
        if removed.status.success() || is_benign_removal_error(&stderr) {
            continue;
        }
        anyhow::bail!(
            "failed to remove Docker network {network}: {}",
            stderr.trim()
        );
    }
    Ok(())
}

fn network_owner(network: &str) -> Result<Option<String>> {
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
    let owner = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok((!owner.is_empty()).then_some(owner))
}

fn has_containers(network: &str, gateway: &str) -> Result<bool> {
    let output = crate::docker::run_docker_output_owned(
        &[
            "ps".to_owned(),
            "-a".to_owned(),
            "--format".to_owned(),
            "{{.Names}}".to_owned(),
            "--filter".to_owned(),
            format!("network={network}"),
        ],
        "failed to list containers of a Docker network",
    )?;
    if !output.status.success() {
        anyhow::bail!(
            "failed to list containers of Docker network {network}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .any(|name| !name.is_empty() && name != gateway))
}

/// A network that is already gone, or that a container joined concurrently,
/// is not a failure of `down`.
fn is_benign_removal_error(stderr: &str) -> bool {
    stderr.contains("not found")
        || stderr.contains("No such")
        || stderr.contains("active endpoints")
}

#[cfg(test)]
mod tests {
    use super::is_benign_removal_error;

    #[test]
    fn gone_or_in_use_networks_are_skipped_and_other_errors_fail() {
        assert!(is_benign_removal_error("Error: No such network: berth_a_b"));
        assert!(is_benign_removal_error(
            "Error response from daemon: error while removing network: network berth_a_b id 1 has active endpoints"
        ));
        assert!(is_benign_removal_error(
            "Error: unable to find network with name or ID berth_a_b: network not found"
        ));
        assert!(!is_benign_removal_error(
            "Cannot connect to the Docker daemon"
        ));
    }
}
