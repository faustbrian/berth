//! cli support port utilities.
//!
//! Contains reusable port helpers used by CLI support and handlers.

mod allocation;
mod bindings;
mod remap;

#[cfg(test)]
pub(crate) use allocation::is_port_available;
#[cfg(test)]
pub(crate) use allocation::random_free_port;
pub(crate) use allocation::{is_port_available_strict, random_unused_port};
pub(crate) use bindings::{
    apply_runtime_binding, collect_service_host_ports, insert_service_host_ports,
    service_host_ports,
};
pub(crate) use remap::remap_random_ports;

pub(crate) const RANDOM_PORT_FALLBACK_HOST: &str = "127.0.0.1";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ServicePortRemap {
    pub(crate) main: bool,
    pub(crate) smtp: bool,
}

impl ServicePortRemap {
    pub(crate) fn was_remapped(&self) -> bool {
        self.main || self.smtp
    }
}

#[cfg(test)]
mod tests {
    use super::{collect_service_host_ports, random_free_port, random_unused_port};
    use crate::config::{Driver, Kind, ServiceConfig};

    #[test]
    fn random_unused_port_avoids_used_ports() {
        let used_port = random_free_port("127.0.0.1").expect("allocate used port");
        let mut used_ports = std::collections::HashSet::new();
        used_ports.insert(("127.0.0.1".to_owned(), used_port));

        let candidate =
            random_unused_port("127.0.0.1", &used_ports).expect("allocate candidate port");
        assert_ne!(candidate, used_port);
    }

    #[test]
    fn collect_service_host_ports_includes_service_port_and_smtp_port() {
        let services = vec![
            service("app", "127.0.0.1", 8080, None),
            service("mail", "127.0.0.1", 25_000, Some(25_501)),
        ];

        let used_ports = collect_service_host_ports(&services);
        assert!(used_ports.contains(&("127.0.0.1".to_owned(), 8080)));
        assert!(used_ports.contains(&("127.0.0.1".to_owned(), 25_000)));
        assert!(used_ports.contains(&("127.0.0.1".to_owned(), 25_501)));
        assert_eq!(used_ports.len(), 3);
    }

    #[test]
    fn service_host_ports_deduplicates_matching_main_and_smtp_port() {
        let service = service("mail", "127.0.0.1", 2_587, Some(2_587));
        let ports = super::service_host_ports(&service);

        assert_eq!(ports.len(), 1);
        assert!(ports.contains(&("127.0.0.1".to_owned(), 2_587)));
    }

    #[test]
    fn remap_random_ports_leaves_smtp_port_unchanged_without_flag() {
        let mut service = service("mail", "127.0.0.1", 25_000, Some(25_001));
        let mut used_ports = collect_service_host_ports(std::slice::from_ref(&service));
        let original_port = service.port;
        let original_smtp_port = service.smtp_port;

        let remapped = super::remap_random_ports(&mut service, &mut used_ports, false)
            .expect("remap service port");

        assert!(remapped.main);
        assert_ne!(service.port, original_port);
        assert!(!remapped.smtp);
        assert_eq!(service.smtp_port, original_smtp_port);
    }

    #[test]
    fn remap_random_ports_remaps_smtp_when_flag_enabled() {
        let mut service = service("mail", "127.0.0.1", 25_000, Some(25_001));
        let mut used_ports = collect_service_host_ports(std::slice::from_ref(&service));
        let original_smtp_port = service.smtp_port;

        let remapped = super::remap_random_ports(&mut service, &mut used_ports, true)
            .expect("remap service ports");

        assert!(remapped.main);
        assert!(remapped.smtp);
        assert_ne!(service.smtp_port, original_smtp_port);
    }

    fn service(name: &str, host: &str, port: u16, smtp_port: Option<u16>) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind: Kind::Cache,
            driver: Driver::Valkey,
            host: host.to_owned(),
            port,
            smtp_port,
            image: "test-image".to_owned(),
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
        }
    }
}
