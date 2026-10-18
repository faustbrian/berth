//! `berth gc`: removes shared servers no instance uses.
//!
//! A server is unused when no instance network has it attached and it holds
//! no tenant. A server berth cannot inspect (stopped, unreachable) is kept
//! and reported, never removed on a guess (ledger decision D9). Removal
//! deletes the container, its data volume and its admin secret (D12).

use anyhow::Result;
use std::path::Path;

use crate::config::{SharedEngine, SharedTarget};

use super::{mysql, postgres, redis, server};

/// What happened to one shared server.
#[derive(Debug)]
pub(crate) struct ServerVerdict {
    pub(crate) server: String,
    pub(crate) engine: &'static str,
    /// `removed`, or `kept` with a reason.
    pub(crate) removed: bool,
    pub(crate) reason: String,
}

/// Removes every unused shared server of this installation.
///
/// # Errors
///
/// Returns an error when Docker cannot list servers or a removal fails.
pub(crate) fn collect_servers() -> Result<Vec<ServerVerdict>> {
    let mut verdicts = Vec::new();
    for target in server::installation_servers()? {
        let _lock = server::lock(&target)?;
        let verdict = match usage(&target) {
            Ok(None) => {
                remove(&target)?;
                ServerVerdict {
                    server: target.server.clone(),
                    engine: target.engine.as_str(),
                    removed: true,
                    reason: "no instance network and no tenant".to_owned(),
                }
            }
            Ok(Some(reason)) | Err(reason) => ServerVerdict {
                server: target.server.clone(),
                engine: target.engine.as_str(),
                removed: false,
                reason,
            },
        };
        verdicts.push(verdict);
    }
    Ok(verdicts)
}

/// `Ok(None)` when the server is unused, `Ok(Some(why))` when it is in use,
/// `Err(why)` when berth cannot tell.
fn usage(target: &SharedTarget) -> std::result::Result<Option<String>, String> {
    let networks = attached_networks(&target.server).map_err(|error| format!("{error:#}"))?;
    if !networks.is_empty() {
        return Ok(Some(format!("attached to {}", networks.join(", "))));
    }
    match crate::docker::inspect_status(&target.server).as_deref() {
        Some("running") => {}
        Some(state) => return Err(format!("{state}; start it or remove it yourself")),
        None => return Err("cannot be inspected".to_owned()),
    }
    let tenants = tenants(target).map_err(|error| format!("cannot list tenants: {error:#}"))?;
    if tenants.is_empty() {
        Ok(None)
    } else {
        Ok(Some(format!("holds {}", tenants.join(", "))))
    }
}

fn attached_networks(server: &str) -> Result<Vec<String>> {
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
        anyhow::bail!(
            "failed to inspect {server}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let networks: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| anyhow::anyhow!("unexpected network list for {server}: {error}"))?;
    Ok(networks
        .as_object()
        .map(|map| {
            map.keys()
                // The engine's default network carries no instance; the
                // server joins it when created.
                .filter(|name| !crate::docker::is_engine_default_network(name))
                .cloned()
                .collect()
        })
        .unwrap_or_default())
}

/// The databases, roles or users the server holds besides its own.
fn tenants(target: &SharedTarget) -> Result<Vec<String>> {
    server::ensure_running(target)?;
    let mut names: Vec<String> = match target.engine {
        SharedEngine::Postgres => {
            let listing = server::admin_sql(target, &postgres::list_names_sql())?;
            let system = |name: &str| {
                matches!(
                    name,
                    "postgres" | "template0" | "template1" | postgres::ADMIN_USER
                ) || name.starts_with("pg_")
            };
            let (databases, roles) =
                super::listing::select_names(&listing, &|name| !system(name), &|name| {
                    !system(name)
                })?;
            databases.into_iter().chain(roles).collect::<Vec<_>>()
        }
        SharedEngine::Mysql | SharedEngine::Mariadb => {
            let listing = server::admin_sql(target, &mysql::list_names_sql())?;
            let system = |name: &str| {
                matches!(
                    name,
                    "information_schema"
                        | "mysql"
                        | "performance_schema"
                        | "sys"
                        | "root"
                        | "mariadb.sys"
                        // The official MariaDB image's own user for its
                        // `healthcheck.sh` (MARIADB_HEALTHCHECK_GRANTS).
                        | "healthcheck"
                ) || name.starts_with("mysql.")
            };
            let (databases, roles) =
                super::listing::select_names(&listing, &|name| !system(name), &|name| {
                    !system(name)
                })?;
            databases.into_iter().chain(roles).collect::<Vec<_>>()
        }
        SharedEngine::Redis | SharedEngine::Valkey => {
            server::admin_sql(target, &redis::list_users_commands())?
                .lines()
                .map(str::trim)
                .filter(|user| !user.is_empty() && *user != "default")
                .map(ToOwned::to_owned)
                .collect()
        }
    };
    names.sort();
    names.dedup();
    Ok(names)
}

fn remove(target: &SharedTarget) -> Result<()> {
    for args in [
        vec!["rm", "-f", "-v", target.server.as_str()],
        vec!["volume", "rm", &format!("{}-data", target.server)],
    ] {
        let args: Vec<String> = args.into_iter().map(ToOwned::to_owned).collect();
        let output =
            crate::docker::run_docker_output_owned(&args, "failed to remove a shared server")?;
        let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
        if !output.status.success() && !stderr.contains("no such") {
            anyhow::bail!("docker {}: {}", args.join(" "), stderr.trim());
        }
    }
    let secrets = crate::home::home_path(&Path::new("shared").join(&target.server))?;
    match std::fs::remove_dir_all(&secrets) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(anyhow::anyhow!(
            "failed to remove {}: {error}",
            secrets.display()
        )),
    }
}
