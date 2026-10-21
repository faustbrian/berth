//! Which shared servers an instance has tenants on, so that purging it
//! visits only those (and never starts another project's stopped server).
//!
//! The record lives at `~/.berth/instances/{project}/{instance}/shared-servers`,
//! one server name per line. It is made, empty, with the instance's
//! directory (`home::instance_dir`), so it is there before any tenant, and a
//! server is noted before a tenant is made on it. An instance without one
//! (made before berth 10.3.0, or whose record was removed by hand), or whose
//! purge could not drop every tenant (a line `*`), is purged from every
//! server, as before. A server it does not list is still checked while it
//! runs, and a stopped one is skipped.

use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::path::Path;

const FILE_NAME: &str = "shared-servers";
const EVERY_SERVER: &str = "*";

fn record_dir(project: &str, instance: &str) -> std::path::PathBuf {
    Path::new("instances").join(project).join(instance)
}

fn lock(project: &str, instance: &str) -> Result<crate::home::HostLock> {
    crate::home::HostLock::acquire(&format!("tenants_{project}_{instance}"))
}

/// Notes a tenant of the instance on `server`.
///
/// # Errors
///
/// Returns an error when the record cannot be read or written.
pub(crate) fn note(project: &str, instance: &str, server: &str) -> Result<()> {
    let _lock = lock(project, instance)?;
    // No record: every server is visited anyway.
    if let Some(mut servers) = read(project, instance)?
        && servers.insert(server.to_owned())
    {
        write(project, instance, &servers)?;
    }
    Ok(())
}

/// Removes `server` from the instance's record: it holds no tenant of the
/// instance any more. A record marked for every server (`*`) stays so.
///
/// # Errors
///
/// Returns an error when the record cannot be read or written.
pub(crate) fn forget(project: &str, instance: &str, server: &str) -> Result<()> {
    let _lock = lock(project, instance)?;
    if let Some(mut servers) = read(project, instance)?
        && servers.remove(server)
    {
        write(project, instance, &servers)?;
    }
    Ok(())
}

/// Marks that the instance may have tenants on any server (a purge that
/// could not drop them all removed the rest of its state).
///
/// # Errors
///
/// Returns an error when the record cannot be written.
pub(crate) fn mark_every_server(project: &str, instance: &str) -> Result<()> {
    let _lock = lock(project, instance)?;
    write(
        project,
        instance,
        &BTreeSet::from([EVERY_SERVER.to_owned()]),
    )
}

/// The servers the instance has tenants on, or `None` when that is not
/// known (every server must be visited).
///
/// # Errors
///
/// Returns an error when the record exists but cannot be read.
pub(crate) fn servers(project: &str, instance: &str) -> Result<Option<BTreeSet<String>>> {
    Ok(read(project, instance)?.filter(|servers| !servers.contains(EVERY_SERVER)))
}

fn read(project: &str, instance: &str) -> Result<Option<BTreeSet<String>>> {
    let path = crate::home::home_path(&record_dir(project, instance).join(FILE_NAME))?;
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(Some(
            text.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToOwned::to_owned)
                .collect(),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
    }
}

fn write(project: &str, instance: &str, servers: &BTreeSet<String>) -> Result<()> {
    let dir = crate::home::private_dir(&record_dir(project, instance))?;
    let staged = dir.join(format!(".{FILE_NAME}.{}", std::process::id()));
    let body: String = servers.iter().map(|server| format!("{server}\n")).collect();
    std::fs::write(&staged, body)
        .with_context(|| format!("failed to write {}", staged.display()))?;
    std::fs::rename(&staged, dir.join(FILE_NAME))
        .with_context(|| format!("failed to write the tenant record in {}", dir.display()))
}
