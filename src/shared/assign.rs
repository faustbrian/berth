//! Points shared-backed services at their server and tenant at config load.

use anyhow::Result;
use std::path::Path;

use crate::config::{Driver, Isolation, ServiceConfig, SharedEngine, SharedTarget};

/// Password shown in dry runs instead of creating a real one.
const DRY_RUN_PASSWORD: &str = "<berth-managed-password>";

/// The engine a service shares, or `None` for a dedicated container.
/// PostgreSQL, MySQL, MariaDB, Redis and Valkey are shared by default (ledger decision 4);
/// `isolation: dedicated` opts out.
#[must_use]
pub(crate) fn shared_engine(service: &ServiceConfig) -> Option<SharedEngine> {
    if service.runtime.isolation == Some(Isolation::Dedicated) {
        return None;
    }
    match service.driver {
        Driver::Postgres => Some(SharedEngine::Postgres),
        Driver::Mysql if crate::docker::is_mariadb_image(&service.image) => {
            Some(SharedEngine::Mariadb)
        }
        Driver::Mysql => Some(SharedEngine::Mysql),
        Driver::Redis => Some(SharedEngine::Redis),
        Driver::Valkey => Some(SharedEngine::Valkey),
        _ => None,
    }
}

/// Backs the service by its tenant in the shared server: the service's
/// container is the server, its credentials are the tenant's.
///
/// # Errors
///
/// Returns an error when the service asks for something a tenant cannot
/// have (`isolation: shared` on an unsupported engine, a host port, authored
/// credentials), or its password cannot be read or created.
pub(crate) fn assign_shared_target(
    service: &mut ServiceConfig,
    project: &str,
    instance: &str,
    env: Option<&str>,
) -> Result<()> {
    let Some(engine) = shared_engine(service) else {
        if service.runtime.isolation == Some(Isolation::Shared) {
            anyhow::bail!(
                "service '{}' asks for `isolation: shared`, which berth supports only for \
                 PostgreSQL, MySQL, MariaDB, Redis and Valkey; remove it or use `isolation: dedicated`",
                service.name
            );
        }
        return Ok(());
    };
    if service.runtime.publish_host_port && env.is_none() {
        anyhow::bail!(
            "service '{}' is a tenant of a shared server and cannot publish a host port; \
             set `isolation: dedicated` to give it a container of its own",
            service.name
        );
    }
    if !service.runtime.authored_container_fields.is_empty() {
        anyhow::bail!(
            "service '{}' sets {}, which configure a container of its own, but it is a tenant \
             of a shared server; remove them or set `isolation: dedicated`",
            service.name,
            service
                .runtime
                .authored_container_fields
                .iter()
                .map(|field| format!("`{field}`"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if service.runtime.authored_credentials {
        anyhow::bail!(
            "service '{}' sets `database`, `username` or `password`, but a shared server's \
             tenant gets generated credentials; remove them or set `isolation: dedicated`",
            service.name
        );
    }
    let installation = crate::home::installation_id()?;
    let mut command = service.command.clone().unwrap_or_default();
    let options = super::engine::server_args(engine, &command);
    command.extend(options);
    let fingerprint = super::naming::fingerprint(engine, &service.image, Some(&command));
    let server = super::naming::server_name(engine, &installation, &fingerprint);
    let tenant = super::naming::tenant_name(project, instance, &service.name, env);
    let password = tenant_password(project, instance, &tenant)?;
    if let Some(root) = service.runtime.root.as_deref() {
        crate::instance::record_root(project, instance, root)?;
    }
    crate::docker::register_secret(&password);

    let username = match engine {
        SharedEngine::Postgres | SharedEngine::Redis | SharedEngine::Valkey => tenant.clone(),
        SharedEngine::Mysql | SharedEngine::Mariadb => {
            super::naming::mysql_user(project, instance, &service.name, env)
        }
    };
    // A Redis tenant is its ACL user and key prefix; its `database` stays the
    // logical database number the preset chose.
    if !matches!(engine, SharedEngine::Redis | SharedEngine::Valkey) {
        service.database = Some(tenant);
    }
    service.username = Some(username);
    service.password = Some(password);
    service.resolved_container_name = Some(server.clone());
    // The tenant is reached inside the instance network; it never has a
    // host port of its own.
    service.runtime.publish_host_port = false;
    service.runtime.shared = Some(SharedTarget {
        engine,
        server,
        image: service.image.clone(),
        command,
        fingerprint,
    });
    Ok(())
}

/// The tenant's password, created once and kept with the instance's berth
/// state until `down --purge`.
fn tenant_password(project: &str, instance: &str, tenant: &str) -> Result<String> {
    if crate::docker::is_dry_run() {
        return Ok(DRY_RUN_PASSWORD.to_owned());
    }
    if cfg!(test) {
        return Ok("0".repeat(64));
    }
    crate::home::read_or_create(
        &Path::new("instances")
            .join(project)
            .join(instance)
            .join("credentials"),
        tenant,
        crate::home::random_hex_secret,
        crate::home::is_hex_secret,
    )
}
