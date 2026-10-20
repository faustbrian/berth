//! Lets an app container call the swarm sources it injects endpoints from
//! (ledger L205): `http://{source app container}:{port}` resolves on the
//! source's endpoint network, which only the source app and its consumers
//! join, so no service alias of either side can shadow the other's.

use anyhow::Result;
use std::path::Path;

use crate::config::ServiceConfig;

/// The endpoint networks the consumer will join, prepared before it starts:
/// each source app runs and is attached to its endpoint network. Test leases
/// call no sources (they fake HTTP), so they skip all of this.
///
/// # Errors
///
/// Returns an error when a source app is not running, or a network cannot be
/// created or joined.
pub(super) fn prepare_dependency_endpoints(
    consumer: &ServiceConfig,
    project_root: &Path,
) -> Result<Vec<String>> {
    let runtime_env = consumer.runtime.env.as_deref();
    if runtime_env.is_some_and(crate::swarm::is_test_lease_env) {
        return Ok(Vec::new());
    }
    let mut networks = Vec::new();
    for (source, network) in
        crate::swarm::resolve_project_dependency_endpoints(project_root, runtime_env)?
    {
        let container = source.container_name()?;
        if crate::docker::inspect_status(&container).as_deref() != Some("running") {
            anyhow::bail!(
                "swarm source app {container} is not running; bring the source up first ({}, or \
                 `berth swarm up` in the workspace)",
                crate::followup::for_service(&source, "up")
            );
        }
        let mut endpoint = source.clone();
        endpoint.runtime.network = Some(network.clone());
        crate::network::ensure_service_network(&endpoint)?;
        connect(&network, &container)?;
        networks.push(network);
    }
    Ok(networks)
}

/// Joins the consumer to the endpoint networks [`prepare_dependency_endpoints`]
/// returned.
///
/// # Errors
///
/// Returns an error when Docker refuses a connection.
pub(super) fn join_dependency_endpoints(container: &str, networks: &[String]) -> Result<()> {
    for network in networks {
        connect(network, container)?;
    }
    Ok(())
}

/// Connects without aliases; already attached is fine.
fn connect(network: &str, container: &str) -> Result<()> {
    let output = crate::docker::run_docker_output(
        &["network", "connect", network, container],
        "failed to join a swarm endpoint network",
    )?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Docker says "already exists", Podman "already connected".
    if output.status.success()
        || stderr.contains("already exists")
        || stderr.contains("already connected")
    {
        return Ok(());
    }
    anyhow::bail!(
        "failed to attach {container} to {network}: {}",
        stderr.trim()
    )
}

/// Puts a (re)started app container back on its own endpoint network when
/// consumers created one: a recreated source would otherwise leave every
/// running consumer unable to resolve it.
///
/// # Errors
///
/// Returns an error when Docker refuses the connection.
pub(super) fn rejoin_own_endpoint(app: &ServiceConfig, container: &str) -> Result<()> {
    let Some(network) = crate::swarm::endpoint_network(app) else {
        return Ok(());
    };
    let exists = crate::docker::run_docker_output(
        &["network", "inspect", "--format", "{{.Name}}", &network],
        "failed to inspect the endpoint network",
    )?
    .status
    .success();
    if exists {
        connect(&network, container)?;
    }
    Ok(())
}
