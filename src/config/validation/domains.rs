//! config validation domains module.
//!
//! Contains config domain resolution logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

use crate::config::{Config, ServiceConfig, domain_names};

/// Resolves generated app domains from project-level config when needed.
pub(super) fn validate_and_resolve_domains(config: &mut Config, project_root: &Path) -> Result<()> {
    let Some(strategy) = config.domain_strategy else {
        return Ok(());
    };

    let base_label = domain_names::base_label_for_project_root(project_root, strategy);

    for service in &mut config.service {
        if service.kind != crate::config::Kind::App {
            service.resolved_domain = None;
            continue;
        }

        if has_authored_domain(service) {
            service.resolved_domain = None;
            continue;
        }

        service.resolved_domain = Some(domain_names::generated_domain(&base_label, &service.name));
    }

    Ok(())
}

fn has_authored_domain(service: &ServiceConfig) -> bool {
    service.domain.as_deref().is_some_and(has_domain_value)
        || service
            .domains
            .as_ref()
            .is_some_and(|domains| domains.iter().any(|domain| has_domain_value(domain)))
}

fn has_domain_value(domain: &str) -> bool {
    !domain.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::validate_and_resolve_domains;
    use crate::config::{Config, DomainStrategy, Driver, Kind, ProjectType, ServiceConfig};
    use std::path::Path;

    fn app(name: &str) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind: Kind::App,
            driver: Driver::Frankenphp,
            image: "dunglas/frankenphp:php8.5".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 8000,
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
            container_name: None,
            resolved_container_name: None,
        }
    }

    #[test]
    fn validate_and_resolve_domains_skips_authored_domains() {
        let mut config = Config {
            schema_version: 1,
            project_type: ProjectType::Project,
            container_prefix: Some("shipit-api".to_owned()),
            domain_strategy: Some(DomainStrategy::Directory),
            service: vec![ServiceConfig {
                domain: Some("authored.berth".to_owned()),
                ..app("app")
            }],
            swarm: Vec::new(),
        };

        validate_and_resolve_domains(&mut config, Path::new("/tmp/my-project"))
            .expect("resolve domains");

        assert_eq!(config.service[0].domain.as_deref(), Some("authored.berth"));
        assert_eq!(config.service[0].resolved_domain, None);
    }
}
