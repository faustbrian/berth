//! `docker run` arguments that attach a container to its instance network.

use crate::config::ServiceConfig;

/// Whether the service's port is published on the host. Services on an
/// instance network are reached inside it (and web apps through gateway
/// routes), so they publish only when asked to; a service without an
/// instance network can only be reached through its host port.
#[must_use]
pub(crate) fn publishes_host_port(service: &ServiceConfig) -> bool {
    service.runtime.publish_host_port || service.runtime.network.is_none()
}

/// Whether the service is reachable on a host port right now: it is set to
/// publish, or its existing container has a host port binding (it was
/// started with `-P`, `--no-publish-all` or `publish: true` by an earlier
/// command). Commands other than `up` do not see `up`'s flags, so they ask
/// the container.
#[must_use]
pub(crate) fn is_published(service: &ServiceConfig) -> bool {
    publishes_host_port(service) || has_host_binding(service)
}

fn has_host_binding(service: &ServiceConfig) -> bool {
    host_binding_port(service).is_some()
}

/// The host port the service's existing container is bound to.
#[must_use]
pub(crate) fn host_binding_port(service: &ServiceConfig) -> Option<u16> {
    let name = service.container_name().ok()?;
    crate::docker::inspect_host_port_binding(&name, service.resolved_container_port())
        .map(|(_, port)| port)
}

/// Makes a service publish the host port its existing container is bound
/// to, if it has one: a recreated container keeps the port an earlier
/// `up -P` assigned. Returns whether it applied.
pub(crate) fn inherit_publication(service: &mut ServiceConfig) -> bool {
    if crate::docker::is_dry_run() {
        return false;
    }
    let Some(port) = host_binding_port(service) else {
        return false;
    };
    service.runtime.publish_host_port = true;
    service.port = port;
    true
}

/// Whether an existing container must be recreated because the service now
/// publishes a host port but the container was started without one (an
/// `up -P` after a plain `up`).
#[must_use]
pub(crate) fn publication_drifted(service: &ServiceConfig) -> bool {
    if crate::docker::is_dry_run() || !publishes_host_port(service) {
        return false;
    }
    let Ok(name) = service.container_name() else {
        return false;
    };
    crate::docker::inspect_status(&name).is_some() && !has_host_binding(service)
}

/// Returns the service as it should be (re)created: when its existing
/// container publishes a host port, the replacement publishes too, so
/// `recreate`, `restart` and daemon recovery keep ports chosen by an earlier
/// `up -P`.
#[must_use]
pub(crate) fn with_existing_publication(
    service: &ServiceConfig,
) -> std::borrow::Cow<'_, ServiceConfig> {
    if publishes_host_port(service) || crate::docker::is_dry_run() || !has_host_binding(service) {
        return std::borrow::Cow::Borrowed(service);
    }
    let mut inherited = service.clone();
    inherit_publication(&mut inherited);
    std::borrow::Cow::Owned(inherited)
}

/// Returns `--network` (and, for long-running services, `--network-alias`)
/// arguments, or nothing for services without an instance network.
#[must_use]
pub(crate) fn network_run_args(service: &ServiceConfig, with_alias: bool) -> Vec<String> {
    let Some(network) = &service.runtime.network else {
        return Vec::new();
    };
    let mut args = vec!["--network".to_owned(), network.clone()];
    if with_alias {
        args.push("--network-alias".to_owned());
        args.push(service.name.clone());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::network_run_args;
    use crate::config::ServiceConfig;

    fn service() -> ServiceConfig {
        serde_json::from_value(serde_json::json!({
            "name": "db", "kind": "database", "driver": "postgres",
            "image": "postgres:17", "host": "127.0.0.1", "port": 5432
        }))
        .expect("service fixture")
    }

    #[test]
    fn services_join_their_instance_network_under_their_name() {
        let mut service = service();
        assert!(
            network_run_args(&service, true).is_empty(),
            "no network, no args"
        );
        service.runtime.network = Some("berth_api_a-1".to_owned());
        assert_eq!(
            network_run_args(&service, true),
            ["--network", "berth_api_a-1", "--network-alias", "db"]
        );
        assert_eq!(
            network_run_args(&service, false),
            ["--network", "berth_api_a-1"]
        );
    }
}
