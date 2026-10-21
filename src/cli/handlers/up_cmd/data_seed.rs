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
        // A `.gz` name reads as gzip; `up` has no `--gzip`.
        let options = database::RestoreOptions {
            reset: false,
            gzip: false,
            origin: database::DumpOrigin::SeedFile,
        };
        // The seed is checked first, so a skip never names one that fails.
        database::check_dump(svc, &seed_path, options)?;
        // One seed at a time per instance: two `up --seed` runs would both
        // find a database empty and both seed it. The lock is the
        // instance's, so a purge removes it with the instance's others.
        let (Some(project), Some(instance)) = (&svc.runtime.project, &svc.runtime.instance) else {
            anyhow::bail!("service `{}` has no berth instance to seed", svc.name);
        };
        let _seeding = crate::home::HostLock::acquire(&format!("seed_{project}_{instance}"))?;
        // A seed fills an empty database; one that already holds objects of
        // its own (an earlier seed, or its own data) is left alone.
        let reseed = crate::followup::for_service(
            svc,
            &format!(
                "restore --service {} --reset --file {}",
                svc.name,
                crate::followup::quote(&seed_path.display().to_string())
            ),
        );
        if database::has_tables(svc)? {
            log::info_if_not_quiet(
                quiet,
                &svc.name,
                &format!(
                    "Skipped data seed from {}: the database already holds tables or other objects \
                     (re-seed with \
                     {reseed})",
                    seed_path.display(),
                ),
            );
            continue;
        }
        // A seed that failed partway leaves objects, so a later `up --seed`
        // would skip the database: the fix is a reset re-seed.
        if let Err(error) = database::restore(svc, &seed_path, options) {
            match database::has_tables(svc) {
                Ok(true) => anyhow::bail!(
                    "{error:#}; once the seed is fixed, re-seed with {reseed} (`up --seed` now \
                     skips this database, which holds what it made)"
                ),
                // What it made may be nothing the count sees (an extension):
                // the re-seed is named for that case too.
                Ok(false) => anyhow::bail!(
                    "{error:#}; once the seed is fixed, run `up --seed` again, or re-seed with \
                     {reseed} if it left something behind (an extension)"
                ),
                // Whether it left anything is unknown: a later `up --seed` may
                // skip the database, so the reset re-seed is named too.
                Err(count) => anyhow::bail!(
                    "{error:#}; whether it left anything is unknown ({count:#}): if `up --seed` \
                     skips this database, re-seed with {reseed}"
                ),
            }
        }
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
