//! cli handlers up cmd data seed module.
//!
//! Contains cli handlers up cmd data seed logic used by Berth command workflows.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::cli::handlers::log;
use crate::{cli, config, database};

pub(super) fn apply_data_seeds(
    config_data: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    profile: Option<&str>,
    workspace_root: &Path,
    quiet: bool,
) -> Result<()> {
    let selected = cli::support::select_up_targets(config_data, service, kind, profile)?;

    for svc in selected {
        if svc.kind != config::Kind::Database {
            continue;
        }
        let Some(seed_file) = svc.seed_file.as_deref() else {
            continue;
        };
        let seed_path = resolve_seed_path(workspace_root, seed_file);
        let gzip = seed_path
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|ext| ext.eq_ignore_ascii_case("gz"));

        database::restore(svc, &seed_path, false, gzip)?;
        log::info_if_not_quiet(
            quiet,
            &svc.name,
            &format!("Applied data seed from {}", seed_path.display()),
        );
    }

    Ok(())
}

/// Resolves seed path using configured inputs and runtime state.
fn resolve_seed_path(workspace_root: &Path, seed_file: &str) -> PathBuf {
    cli::support::resolve_workspace_path(workspace_root, seed_file)
}
