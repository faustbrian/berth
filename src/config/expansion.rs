//! config expansion module.
//!
//! Contains config expansion logic used by Berth command workflows.

use anyhow::Result;

use super::{Config, ProjectType, RawConfig, RawServiceConfig, ServiceConfig, SwarmTarget};

mod ports;
mod service;

pub(super) fn expand_raw_config(raw: RawConfig) -> Result<Config> {
    let schema_version = raw.schema_version.unwrap_or(1);
    if schema_version != 1 {
        // `parse_project_file` always converts to schema 1.
        anyhow::bail!("internal error: converted config has schema_version {schema_version}");
    }
    let project_type = raw.project_type.unwrap_or(ProjectType::Project);

    let mut services: Vec<ServiceConfig> = raw
        .service
        .into_iter()
        .map(expand_raw_service)
        .collect::<Result<Vec<_>>>()?;

    ports::assign_missing_ports(&mut services)?;

    Ok(Config {
        schema_version,
        project_type,
        container_prefix: raw.container_prefix,
        domain_strategy: raw.domain_strategy,
        service: services,
        swarm: raw
            .swarm
            .into_iter()
            .map(|target| {
                let git = match target.git.len() {
                    0 => None,
                    1 => Some(super::SwarmGit {
                        repo: target.git[0].repo.clone(),
                        branch: target.git[0].branch.clone(),
                    }),
                    _ => {
                        anyhow::bail!(
                            "swarm target '{}' lists {} `git` entries; keep one: `git: [{{repo: \
                             <url>, branch: <name>}}]`",
                            target.name,
                            target.git.len()
                        );
                    }
                };

                Ok(SwarmTarget {
                    name: target.name,
                    root: target.root,
                    depends_on: target.depends_on,
                    inject_env: target.inject_env,
                    git,
                })
            })
            .collect::<Result<Vec<_>>>()?,
    })
}

pub(super) fn expand_raw_service(raw: RawServiceConfig) -> Result<ServiceConfig> {
    service::expand_raw_service(raw)
}

#[cfg(test)]
mod tests {
    use super::{RawServiceConfig, expand_raw_service};
    use crate::config::{Driver, Kind};

    #[test]
    fn expand_raw_service_preserves_octane_worker_settings() -> anyhow::Result<()> {
        let service = expand_raw_service(RawServiceConfig {
            preset: None,
            name: Some("app".to_owned()),
            kind: Some(Kind::App),
            driver: Some(Driver::Frankenphp),
            image: Some("dunglas/frankenphp:php8.5".to_owned()),
            host: Some("127.0.0.1".to_owned()),
            port: Some(8080),
            database: None,
            username: None,
            password: None,
            bucket: None,
            access_key: None,
            secret_key: None,
            api_key: None,
            region: None,
            scheme: None,
            domain: Some("acme.berth".to_owned()),
            domains: None,
            routes: None,
            container_port: Some(80),
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
            localhost_tls: None,
            octane: Some(true),
            octane_workers: Some(6),
            octane_max_requests: Some(500),
            php_extensions: None,
            trust_container_ca: None,
            env_mapping: None,
            javascript: None,
            container_name: None,
            publish: None,
            cpus: None,
            memory: None,
            isolation: None,
        })?;

        assert!(service.octane);
        assert_eq!(service.octane_workers, Some(6));
        assert_eq!(service.octane_max_requests, Some(500));

        Ok(())
    }
}
