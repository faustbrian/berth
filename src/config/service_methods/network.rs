//! Service networking helpers.

use super::ServiceConfig;

const PORT_BINDING_LOOPBACK_HOST: &str = "127.0.0.1";
const PORT_BINDING_ANY_HOST: &str = "0.0.0.0";
const PORT_BINDING_ANY_HOST_IPV6: &str = "::";

/// Canonicalizes a host string for host-port allocation and conflict detection.
pub(crate) fn normalize_host_for_port_allocation(host: &str) -> String {
    let normalized = host.trim();
    let normalized = if normalized.starts_with('[') && normalized.ends_with(']') {
        &normalized[1..normalized.len() - 1]
    } else {
        normalized
    };

    if normalized.is_empty() || normalized == PORT_BINDING_ANY_HOST {
        return PORT_BINDING_ANY_HOST.to_owned();
    }

    if normalized == PORT_BINDING_ANY_HOST_IPV6 {
        return PORT_BINDING_ANY_HOST_IPV6.to_owned();
    }

    if normalized.eq_ignore_ascii_case("localhost") {
        return PORT_BINDING_LOOPBACK_HOST.to_owned();
    }

    normalized.to_ascii_lowercase()
}

#[must_use]
pub(crate) fn is_unspecified_port_allocation_host(host: &str) -> bool {
    matches!(
        normalize_host_for_port_allocation(host).as_str(),
        PORT_BINDING_ANY_HOST | PORT_BINDING_ANY_HOST_IPV6,
    )
}

impl ServiceConfig {
    /// Returns whether runtime access should resolve through host-gateway alias.
    #[must_use]
    pub fn uses_host_gateway_alias(&self) -> bool {
        normalize_host_for_port_allocation(&self.host) == PORT_BINDING_LOOPBACK_HOST
    }

    /// Returns a canonical host value for port allocation and conflict checks.
    #[must_use]
    pub(crate) fn normalized_host_for_ports(&self) -> String {
        normalize_host_for_port_allocation(&self.host)
    }
}

#[cfg(test)]
mod tests {
    use super::ServiceConfig;
    use super::normalize_host_for_port_allocation;
    use crate::config::is_unspecified_port_allocation_host;

    #[test]
    fn normalize_host_for_port_allocation_normalizes_loopback_host() {
        assert_eq!(
            normalize_host_for_port_allocation(" localhost "),
            "127.0.0.1"
        );
    }

    #[test]
    fn normalize_host_for_port_allocation_normalizes_empty_and_any_host() {
        assert_eq!(normalize_host_for_port_allocation(""), "0.0.0.0");
        assert_eq!(normalize_host_for_port_allocation("::"), "::");
        assert_eq!(normalize_host_for_port_allocation("0.0.0.0"), "0.0.0.0");
    }

    #[test]
    fn normalize_host_for_port_allocation_strips_brackets() {
        assert_eq!(
            normalize_host_for_port_allocation("[127.0.0.1]"),
            "127.0.0.1"
        );
        assert_eq!(normalize_host_for_port_allocation("[::]"), "::");
    }

    #[test]
    fn host_allocation_any_host_classification() {
        assert!(is_unspecified_port_allocation_host("0.0.0.0"));
        assert!(is_unspecified_port_allocation_host("::"));
        assert!(is_unspecified_port_allocation_host("[::]"));
        assert!(!is_unspecified_port_allocation_host("[::1]"));
        assert!(!is_unspecified_port_allocation_host("localhost"));
    }

    #[test]
    fn host_gateway_alias_uses_normalized_host() {
        let service = ServiceConfig {
            name: "db".to_owned(),
            kind: crate::config::Kind::Database,
            driver: crate::config::Driver::Mysql,
            image: "mysql:8.1".to_owned(),
            host: " LOCALHOST ".to_owned(),
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
            routes: None,
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
            container_name: None,
            resolved_container_name: None,
            runtime: Default::default(),
        };

        assert!(service.uses_host_gateway_alias());
    }
}
