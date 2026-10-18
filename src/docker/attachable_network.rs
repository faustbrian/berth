//! `docker run` arguments that let a container join other networks later.

use crate::config::ContainerEngine;

/// Podman's default bridge network.
const PODMAN_DEFAULT_NETWORK: &str = "podman";

/// Returns the `--network` arguments for a container that berth starts on
/// no network of its own and attaches to instance networks afterwards (the
/// shared servers and the gateway).
///
/// Docker starts such a container on its default bridge, which accepts
/// `network connect`. Rootless Podman gives it a private slirp4netns or
/// pasta stack instead, and `network connect` then fails with
/// `"slirp4netns" is not supported: invalid network mode` (Podman 4.9 on
/// Ubuntu 24.04), so on Podman it starts on Podman's own bridge.
pub(crate) fn attachable_network_args() -> Vec<String> {
    match super::container_engine() {
        ContainerEngine::Docker => Vec::new(),
        ContainerEngine::Podman => vec!["--network".to_owned(), PODMAN_DEFAULT_NETWORK.to_owned()],
    }
}

/// Whether `network` is an engine's default network (Docker's `bridge`,
/// Podman's `podman`), which carries no instance.
pub(crate) fn is_engine_default_network(network: &str) -> bool {
    matches!(network, "bridge" | PODMAN_DEFAULT_NETWORK)
}

#[cfg(test)]
mod tests {
    use super::{attachable_network_args, is_engine_default_network};
    use crate::config::ContainerEngine;

    #[test]
    fn only_podman_needs_an_explicit_bridge() {
        assert!(attachable_network_args().is_empty());
        crate::docker::with_container_engine(ContainerEngine::Podman, || {
            assert_eq!(attachable_network_args(), ["--network", "podman"]);
        });
    }

    #[test]
    fn default_networks_of_both_engines_carry_no_instance() {
        assert!(is_engine_default_network("bridge"));
        assert!(is_engine_default_network("podman"));
        assert!(!is_engine_default_network("berth_shop_a-1b2c3d"));
    }
}
