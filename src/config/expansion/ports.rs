//! config expansion ports module.
//!
//! Contains config expansion ports logic used by Berth command workflows.

use anyhow::{Result, anyhow};
use std::collections::HashSet;

use super::super::{Driver, ServiceConfig};

pub(super) fn assign_missing_ports(services: &mut [ServiceConfig]) -> Result<()> {
    let mut used: HashSet<(String, u16)> = HashSet::new();
    for service in services.iter() {
        let host = service.normalized_host_for_ports();

        if service.port != 0 {
            used.insert((host.clone(), service.port));
        }
        if let Some(smtp_port) = service.smtp_port {
            used.insert((host, smtp_port));
        }
    }

    for service in services {
        if service.port != 0 {
            continue;
        }

        let host = service.normalized_host_for_ports();

        let mut candidate = preferred_start_port(service.driver);
        while used.contains(&(host.clone(), candidate)) {
            candidate = candidate
                .checked_add(1)
                .ok_or_else(|| anyhow!("no available port for service '{}'", service.name))?;
            if candidate < 1024 {
                anyhow::bail!("no available port for service '{}'", service.name);
            }
        }
        service.port = candidate;
        used.insert((host.clone(), candidate));

        if service.driver == Driver::Mailhog && service.smtp_port.is_none() {
            let mut smtp_candidate = candidate
                .checked_add(1000)
                .ok_or_else(|| anyhow!("no available smtp port for service '{}'", service.name))?;
            while used.contains(&(host.clone(), smtp_candidate)) {
                smtp_candidate = smtp_candidate.checked_add(1).ok_or_else(|| {
                    anyhow!("no available smtp port for service '{}'", service.name)
                })?;
            }
            service.smtp_port = Some(smtp_candidate);
            used.insert((host, smtp_candidate));
        }
    }

    Ok(())
}

const fn preferred_start_port(driver: Driver) -> u16 {
    match driver {
        Driver::Mongodb => 27017,
        Driver::Memcached => 11211,
        Driver::Mysql => 33060,
        Driver::Postgres => 54320,
        Driver::Sqlserver => 14330,
        Driver::Redis | Driver::Valkey | Driver::Dragonfly => 6380,
        Driver::Minio | Driver::Rustfs => 9000,
        Driver::Garage => 3900,
        Driver::Localstack => 4566,
        Driver::Opensearch | Driver::Elasticsearch => 9200,
        Driver::Meilisearch => 7700,
        Driver::Typesense => 8108,
        Driver::Frankenphp => 33065,
        Driver::Gotenberg => 33066,
        Driver::Mailhog => 33067,
        Driver::Reverb => 33068,
        Driver::Horizon => 33069,
        Driver::Scheduler => 33071,
        Driver::Dusk => 33070,
        Driver::Rabbitmq => 5672,
        Driver::Soketi => 6001,
    }
}
