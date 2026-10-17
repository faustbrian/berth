//! Port-assignment policy for random-port startup paths.

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::Path;

use crate::cli::args::PortStrategyArg;
use crate::{cli, config};
mod stable;
pub(super) use stable::effective_port_seed;
use stable::stable_port_offset;

pub(super) fn explicit_port_service_names(
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<(HashSet<String>, HashSet<String>)> {
    let raw_config =
        config::load_raw_config_with(config::RawConfigPathOptions::new(config_path, project_root))?;
    let mut explicit_port_services = HashSet::new();
    let mut explicit_smtp_services = HashSet::new();

    for raw_service in &raw_config.service {
        if let Some(name) = &raw_service.name
            && raw_service.port.is_some()
        {
            explicit_port_services.insert(name.clone());
        }
        if raw_service.smtp_port.is_some()
            && let Some(name) = &raw_service.name
        {
            explicit_smtp_services.insert(name.clone());
        }
    }

    Ok((explicit_port_services, explicit_smtp_services))
}

/// Returns whether this service port should be randomized.
pub(super) fn should_randomize_port(
    explicit_port_services: &HashSet<String>,
    service_name: &str,
    force_random_ports: bool,
) -> bool {
    force_random_ports || !explicit_port_services.contains(service_name)
}

pub(super) fn assign_runtime_port(
    service: &config::ServiceConfig,
    strategy: PortStrategyArg,
    seed: &str,
    used_ports: &mut HashSet<(String, u16)>,
    field_name: &str,
) -> Result<u16> {
    match strategy {
        PortStrategyArg::Random => cli::support::random_unused_port(&service.host, used_ports)
            .with_context(|| {
                format!(
                    "failed to allocate non-conflicting random port for '{}'",
                    service.name
                )
            }),
        PortStrategyArg::Stable => assign_stable_port(service, seed, used_ports, field_name),
    }
}

fn assign_stable_port(
    service: &config::ServiceConfig,
    seed: &str,
    used_ports: &HashSet<(String, u16)>,
    field_name: &str,
) -> Result<u16> {
    const RANGE_START: u16 = 20_000;
    const RANGE_SIZE: u16 = 40_000;
    let base = stable_port_offset(seed, &service.name, field_name, RANGE_SIZE);

    for offset in 0..RANGE_SIZE {
        let candidate = RANGE_START + ((base + offset) % RANGE_SIZE);
        if used_ports.contains(&(service.host.clone(), candidate)) {
            continue;
        }
        if cli::support::is_port_available_strict(&service.host, candidate) {
            return Ok(candidate);
        }
    }

    anyhow::bail!(
        "failed to allocate deterministic port for '{}' using stable strategy",
        service.name
    )
}

#[cfg(test)]
mod tests {
    use super::should_randomize_port;
    use super::stable::{effective_port_seed, stable_port_offset};
    use std::collections::HashSet;
    use std::path::Path;

    /// Returns whether explicit service filters disable port randomization.
    #[test]
    fn should_randomize_port_skips_explicit_service() {
        let explicit = HashSet::from([String::from("db")]);
        assert!(!should_randomize_port(&explicit, "db", false));
        assert!(should_randomize_port(&explicit, "cache", false));
        assert!(should_randomize_port(&explicit, "db", true));
    }

    #[test]
    fn stable_port_offset_is_deterministic() {
        let first = stable_port_offset("seed", "db", "port", 40_000);
        let second = stable_port_offset("seed", "db", "port", 40_000);
        assert_eq!(first, second);
    }

    #[test]
    fn stable_port_offset_changes_with_seed_scope() {
        let left = stable_port_offset("/ws/a::local::shared", "db", "port", 40_000);
        let right = stable_port_offset("/ws/b::local::shared", "db", "port", 40_000);
        assert_ne!(left, right);
    }

    #[test]
    fn shared_user_seed_is_scoped_by_workspace() {
        let seed_a = effective_port_seed(Path::new("/tmp/ws-a"), Some("local"), Some("shared"));
        let seed_b = effective_port_seed(Path::new("/tmp/ws-b"), Some("local"), Some("shared"));

        let offset_a = stable_port_offset(&seed_a, "db", "port", 40_000);
        let offset_b = stable_port_offset(&seed_b, "db", "port", 40_000);

        assert_ne!(offset_a, offset_b);
    }
}
