//! cli handlers restore cmd module.
//!
//! Contains cli handlers restore cmd logic used by Berth command workflows.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::{cli, config, database};

pub(crate) struct HandleRestoreOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) file: Option<&'a PathBuf>,
    pub(crate) reset: bool,
    pub(crate) migrate: bool,
    pub(crate) schema_dump: bool,
    pub(crate) gzip: bool,
    pub(crate) project_root: Option<&'a Path>,
    pub(crate) config_path: Option<&'a Path>,
}

pub(crate) fn handle_restore(
    config: &config::Config,
    options: HandleRestoreOptions<'_>,
) -> Result<()> {
    let svc = config::resolve_service(config, options.service)?;
    cli::support::ensure_sql_service(svc, "restore")?;

    match options.file {
        Some(path) => database::restore(
            svc,
            path,
            database::RestoreOptions {
                reset: options.reset,
                gzip: options.gzip,
                origin: database::DumpOrigin::FileFlag,
            },
        )?,
        None => database::restore_stdin(
            svc,
            database::RestoreOptions {
                reset: options.reset,
                gzip: options.gzip,
                origin: database::DumpOrigin::Stdin,
            },
        )?,
    }

    if options.migrate || options.schema_dump {
        database::run_laravel_post_restore(database::PostRestoreOptions::new(
            options.migrate,
            options.schema_dump,
            svc,
            options.project_root,
            options.config_path,
        ))?;
    }

    Ok(())
}
