//! What berth's lifecycle commands do for a shared-backed service: they act
//! on the instance's tenant and its network attachment, never on the shared
//! server container, which other instances use.

use anyhow::Result;

use crate::config::{ServiceConfig, SharedEngine, SharedTarget};
use crate::output::{self, LogLevel, Persistence};

use super::{postgres, server};

fn target(service: &ServiceConfig) -> Result<&SharedTarget> {
    service
        .runtime
        .shared
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("service '{}' is not shared", service.name))
}

/// Ensures the shared server runs, the tenant exists with the current
/// password, and the server answers as `{service}` on the instance network.
///
/// # Errors
///
/// Returns an error when the server cannot be readied or the tenant
/// provisioned.
pub(crate) fn up(service: &ServiceConfig) -> Result<()> {
    let target = target(service)?;
    let tenant = service.database.clone().unwrap_or_default();
    if crate::docker::is_dry_run() {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!(
                "[dry-run] Ensure shared {} server {} and tenant {tenant}",
                target.engine.as_str(),
                target.server
            ),
            Persistence::Transient,
        );
        return Ok(());
    }
    {
        let _lock = server::lock(target)?;
        server::ensure_running(target)?;
        let password = service.password.clone().unwrap_or_default();
        match target.engine {
            SharedEngine::Postgres => {
                let base = match (&service.runtime.project, &service.runtime.instance) {
                    (Some(project), Some(instance)) => {
                        super::naming::tenant_base(project, instance)
                    }
                    _ => anyhow::bail!("service '{}' has no instance identity", service.name),
                };
                server::psql(
                    target,
                    &postgres::provision_tenant_sql(&tenant, &password, &base),
                )?;
            }
        }
        attach(service, target)?;
    }
    output::event(
        &service.name,
        LogLevel::Success,
        &format!("Tenant {tenant} ready on shared server {}", target.server),
        Persistence::Persistent,
    );
    Ok(())
}

/// Fails for a shared-backed service: `verb` would act on the server that
/// other instances use.
///
/// # Errors
///
/// Returns the refusal, naming `isolation: dedicated`.
pub(crate) fn refuse_server_operation(service: &ServiceConfig, verb: &str) -> Result<()> {
    let Some(target) = &service.runtime.shared else {
        return Ok(());
    };
    anyhow::bail!(
        "service '{}' is a tenant of shared server {}, which other instances use; berth \
         does not {verb} it. Set `isolation: dedicated` for a container of the instance's own",
        service.name,
        target.server
    )
}

/// Detaches the service from its instance network, which is all `down`,
/// `stop`, `kill` and `rm` do for a shared-backed service. The server stays
/// on the network while another shared service of the instance still uses
/// it under its own name.
///
/// # Errors
///
/// Returns an error when Docker fails for a reason other than the server or
/// network being gone or not attached.
pub(crate) fn detach(service: &ServiceConfig) -> Result<()> {
    let target = target(service)?;
    let Some(network) = &service.runtime.network else {
        return Ok(());
    };
    if crate::docker::is_dry_run() {
        crate::docker::print_docker_command(&[
            "network".to_owned(),
            "disconnect".to_owned(),
            network.clone(),
            target.server.clone(),
        ]);
        return Ok(());
    }
    let _lock = server::lock(target)?;
    let Some(current) = aliases(&target.server, network)? else {
        return Ok(());
    };
    let remaining: Vec<String> = current
        .iter()
        .filter(|alias| **alias != service.name)
        .cloned()
        .collect();
    set_aliases(&target.server, network, Some(&current), &remaining)
}

/// Drops the service's tenant (its database, the `_test_N` databases
/// ParaTest made for it, and its role). The tenant is recreated by the next
/// `up`; its password stays until the instance state is purged.
///
/// # Errors
///
/// Returns an error when the server cannot be reached or refuses the drop.
pub(crate) fn drop_tenant(service: &ServiceConfig) -> Result<()> {
    let target = target(service)?;
    let tenant = service.database.clone().unwrap_or_default();
    drop_matching(&service.name, target, |name| {
        name == tenant
            || name
                .strip_prefix(&tenant)
                .is_some_and(|rest| rest.starts_with("_test_"))
    })
}

/// Drops every tenant of the instance on every shared server of this
/// installation, across all its runtime environments, including servers the
/// current config no longer uses (an image changed since the tenant was
/// made).
///
/// # Errors
///
/// Returns the first failure after trying every server.
pub(crate) fn drop_instance_tenants(project: &str, instance: &str) -> Result<()> {
    if crate::docker::is_dry_run() {
        output::event(
            "down",
            LogLevel::Info,
            &format!("[dry-run] Drop every tenant of {instance} on the shared servers"),
            Persistence::Transient,
        );
        return Ok(());
    }
    let mut failures = Vec::new();
    for target in server::installation_servers()? {
        let dropped = drop_matching("down", &target, |name| {
            super::naming::belongs_to_instance(name, project, instance)
        });
        if let Err(error) = dropped {
            failures.push(format!("{error:#}"));
        }
    }
    match failures.into_iter().next() {
        None => Ok(()),
        Some(first) => anyhow::bail!("failed to drop tenants of {instance}: {first}"),
    }
}

/// Drops the databases and roles whose names `matches`, and every database
/// a matching role owns (a tenant may have created one outside its
/// namespace; it would otherwise keep its role from being dropped).
fn drop_matching(label: &str, target: &SharedTarget, matches: impl Fn(&str) -> bool) -> Result<()> {
    if crate::docker::is_dry_run() {
        output::event(
            label,
            LogLevel::Info,
            &format!(
                "[dry-run] Drop tenant databases and roles on {}",
                target.server
            ),
            Persistence::Transient,
        );
        return Ok(());
    }
    // A server that was never created holds no tenant.
    if !server_exists(&target.server)? {
        return Ok(());
    }
    let _lock = server::lock(target)?;
    server::ensure_running(target)?;
    match target.engine {
        SharedEngine::Postgres => {
            let listing = server::psql(target, &postgres::list_names_sql())?;
            let (databases, roles) = postgres::parse_names(&listing, &matches)?;
            if databases.is_empty() && roles.is_empty() {
                return Ok(());
            }
            server::psql(target, &postgres::drop_sql(&databases, &roles))?;
            let mut dropped = databases;
            dropped.extend(
                roles
                    .into_iter()
                    .filter(|role| !dropped.contains(role))
                    .collect::<Vec<_>>(),
            );
            output::event(
                label,
                LogLevel::Success,
                &format!(
                    "Dropped {} from shared server {}",
                    dropped.join(", "),
                    target.server
                ),
                Persistence::Persistent,
            );
        }
    }
    Ok(())
}

/// The service's status as its instance sees it: the server's status while
/// it is attached to the instance network, `exited` once the instance has
/// detached (`down`, `stop`), and `None` when the server does not exist.
#[must_use]
pub(crate) fn status(service: &ServiceConfig) -> Option<String> {
    let target = service.runtime.shared.as_ref()?;
    let server = crate::docker::inspect_status(&target.server)?;
    if crate::docker::is_dry_run() {
        return Some(server);
    }
    let Some(network) = &service.runtime.network else {
        return Some(server);
    };
    let attached = aliases(&target.server, network)
        .ok()?
        .is_some_and(|aliases| aliases.contains(&service.name));
    Some(if attached {
        server
    } else {
        "exited".to_owned()
    })
}

fn server_exists(server: &str) -> Result<bool> {
    let output = crate::docker::run_docker_output(
        &["inspect", "--type", "container", server],
        "failed to inspect the shared server",
    )?;
    Ok(output.status.success())
}

/// Puts the server on the instance network under the service's name, next
/// to the names of the instance's other shared services on that server.
/// Must hold the server's lock.
fn attach(service: &ServiceConfig, target: &SharedTarget) -> Result<()> {
    let Some(network) = &service.runtime.network else {
        return Ok(());
    };
    let current = aliases(&target.server, network)?;
    let mut desired = current.clone().unwrap_or_default();
    if !desired.contains(&service.name) {
        desired.push(service.name.clone());
    }
    set_aliases(&target.server, network, current.as_deref(), &desired)
}

/// The names the server answers to on `network`, or `None` when it is not
/// attached. Docker adds the container's own name and short ID; they are
/// not service names.
fn aliases(server: &str, network: &str) -> Result<Option<Vec<String>>> {
    let output = crate::docker::run_docker_output(
        &[
            "inspect",
            "--format",
            "{{json .NetworkSettings.Networks}}",
            server,
        ],
        "failed to inspect the shared server",
    )?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // A server that does not exist (never created, or removed) is
        // attached nowhere.
        if stderr.to_ascii_lowercase().contains("no such") {
            return Ok(None);
        }
        anyhow::bail!(
            "failed to inspect shared server {server}: {}",
            stderr.trim()
        );
    }
    let networks: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| anyhow::anyhow!("unexpected network list for {server}: {error}"))?;
    let Some(endpoint) = networks.get(network) else {
        return Ok(None);
    };
    let names = endpoint
        .get("Aliases")
        .and_then(serde_json::Value::as_array)
        .map(|aliases| {
            aliases
                .iter()
                .filter_map(serde_json::Value::as_str)
                .filter(|alias| *alias != server && !is_container_id(alias))
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default();
    Ok(Some(names))
}

fn is_container_id(alias: &str) -> bool {
    alias.len() == 12 && alias.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Makes the server's endpoint on `network` carry exactly `desired`
/// (detached when empty). Docker cannot change an endpoint's aliases, so a
/// changed set reconnects the server.
fn set_aliases(
    server: &str,
    network: &str,
    current: Option<&[String]>,
    desired: &[String],
) -> Result<()> {
    if let Some(current) = current {
        let unchanged =
            current.len() == desired.len() && desired.iter().all(|alias| current.contains(alias));
        if unchanged && !desired.is_empty() {
            return Ok(());
        }
        let output = crate::docker::run_docker_output(
            &["network", "disconnect", "--force", network, server],
            "failed to detach the shared server",
        )?;
        let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
        if !output.status.success()
            && !stderr.contains("is not connected")
            && !stderr.contains("not found")
        {
            anyhow::bail!(
                "failed to detach {server} from {network}: {}",
                stderr.trim()
            );
        }
    }
    if desired.is_empty() {
        return Ok(());
    }
    let mut args = vec!["network".to_owned(), "connect".to_owned()];
    for alias in desired {
        args.extend(["--alias".to_owned(), alias.clone()]);
    }
    args.extend([network.to_owned(), server.to_owned()]);
    let output =
        crate::docker::run_docker_output_owned(&args, "failed to attach the shared server")?;
    if !output.status.success() {
        anyhow::bail!(
            "failed to attach {server} to {network}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}
