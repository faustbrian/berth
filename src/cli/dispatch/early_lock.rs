//! Marks the instance in use before anything of it is read from the home.

use anyhow::{Context, Result};

/// Holds the instance's in-use lock for the whole command, taken before its
/// config is loaded: loading reads and creates the instance's credentials
/// and root record in the berth home (`env`, `config validate` and the
/// other commands that never touch Docker included), which a concurrent
/// reap deletes. The identity comes from the project file alone (its
/// `project` and the instance derived from the checkout or `--instance`),
/// so nothing of the instance is read until the lock is held. `None` for
/// dry runs, which write nothing, and unit tests, which must never mark the
/// developer's own instances.
///
/// # Errors
///
/// Returns an error when the project file or instance cannot be resolved,
/// or the lock cannot be taken.
pub(super) fn hold_before_load(
    context: &super::context::CliDispatchContext<'_>,
) -> Result<Option<EarlyLock>> {
    if cfg!(test) || crate::docker::is_dry_run() {
        return Ok(None);
    }
    let raw = crate::config::load_raw_config_with(
        crate::config::LoadConfigPathOptions::new(context.config_path(), context.project_root())
            .with_runtime_env(context.runtime_env()),
    )?;
    let Some(project) = raw.container_prefix.clone() else {
        // Loading the config reports the missing `project`.
        return Ok(None);
    };
    let root = crate::config::project_root_with(crate::config::ProjectRootPathOptions::new(
        context.config_path(),
        context.project_root(),
    ))?;
    let identity = crate::instance::resolve_instance(&root, context.instance())
        .context("failed to resolve the instance to mark in use")?;
    let in_use = crate::instance::hold_in_use(&project, &identity.id)?;
    Ok(Some(EarlyLock {
        project,
        instance: identity.id,
        _in_use: in_use,
    }))
}

/// The in-use lock taken before the config was loaded, with the identity it
/// was taken for.
pub(super) struct EarlyLock {
    pub(super) project: String,
    pub(super) instance: String,
    _in_use: crate::instance::InUse,
}
