//! cli handlers url cmd module.
//!
//! Contains cli handlers url cmd logic used by Berth command workflows.

use anyhow::Result;
use colored::Colorize;

use crate::cli::handlers::serialize;
use crate::{cli, config};

pub(crate) fn handle_url(
    config: &config::Config,
    service: Option<&str>,
    format: &str,
    kind: Option<config::Kind>,
    driver: Option<config::Driver>,
) -> Result<()> {
    let selected = selected_url_services(config, service, kind, driver)?;
    match format {
        "json" => {
            let urls: Vec<serde_json::Value> = selected
                .iter()
                .map(|svc| {
                    serde_json::json!({
                        "name": svc.name,
                        "url": crate::network::host_url(svc),
                        "published": crate::network::host_url(svc).is_some(),
                    })
                })
                .collect();
            serialize::print_json_pretty(&urls)?;
        }
        _ => {
            if service.is_some() {
                for svc in selected {
                    let url = crate::network::host_url(svc).ok_or_else(|| {
                        anyhow::anyhow!(
                            "service '{}' is {}",
                            svc.name,
                            crate::network::publish_hint()
                        )
                    })?;
                    println!("{url}");
                }
            } else {
                for svc in selected {
                    let url = crate::network::host_url(svc)
                        .unwrap_or_else(|| format!("({})", crate::network::publish_hint()));
                    println!("{}: {url}", svc.name.bold());
                }
            }
        }
    }
    Ok(())
}

fn selected_url_services<'a>(
    config: &'a config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    driver: Option<config::Driver>,
) -> Result<Vec<&'a config::ServiceConfig>> {
    if let Some(name) = service {
        let service = config::find_service(config, name)?;
        if cli::support::matches_filter(service, kind, driver) {
            return Ok(vec![service]);
        }
        return Ok(Vec::new());
    }

    Ok(cli::support::filter_services(&config.service, kind, driver))
}

#[cfg(test)]
mod tests {
    use crate::{
        cli::handlers::url_cmd,
        config::{Config, Driver, Kind, ServiceConfig},
    };

    fn service(name: &str, kind: Kind, driver: Driver) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind,
            driver,
            image: "php".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 9000,
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
        }
    }

    fn config() -> Config {
        Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![service("api", Kind::App, Driver::Frankenphp)],
            swarm: Vec::new(),
        }
    }

    #[test]
    fn url_handler_with_empty_config_prints_json_array() {
        let config = Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: Vec::new(),
            swarm: Vec::new(),
        };
        assert!(url_cmd::handle_url(&config, None, "json", None, None).is_ok());
    }

    #[test]
    fn url_handler_rejects_missing_service() {
        let config = config();
        let err = url_cmd::handle_url(&config, Some("missing"), "", None, None);
        assert!(err.is_err());
    }
}
