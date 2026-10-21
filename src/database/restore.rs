//! database restore module.
//!
//! Contains database restore logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

use prepare::prepare_restore;

pub(crate) use dump_file::DumpOrigin;
pub(crate) use options::RestoreOptions;

mod dump_file;
mod extension_tables;
mod from_file;
mod from_stdin;
mod has_tables;
mod options;
mod prepare;
mod process;
mod psql_errors;
mod statement_lines;
mod stdin_spool;

pub(crate) fn restore(
    service: &crate::config::ServiceConfig,
    file_path: &Path,
    options: RestoreOptions,
) -> Result<()> {
    // The dump is opened and checked first: `--reset` drops the database.
    let read = dump_file::DumpRead {
        gzip_flag: options.gzip,
        gz_name: from_file::is_gzip_path(file_path),
        origin: options.origin,
        shared_postgres: shared_postgres(service),
        postgres: service.driver == crate::config::Driver::Postgres,
        // As the restore client names it (`piped_client_args`).
        database: service.database.as_deref().unwrap_or("app"),
    };
    let (total_bytes, reader) = dump_file::check_dump_file(file_path, read)?;
    // So is the client: `--reset` must not drop a database it cannot refill.
    crate::docker::ensure_client_reachable(service, options.reset)?;
    prepare_for_restore(service, options.reset)?;

    // A gzip dump's decompressed size is unknown, so it has no percentage.
    let total = (!read.gzip_flag && !read.gz_name).then_some(total_bytes);
    from_file::restore_from_file(service, file_path, total, reader, options)
}

pub(crate) fn restore_stdin(
    service: &crate::config::ServiceConfig,
    options: RestoreOptions,
) -> Result<()> {
    // It is read and checked in full first: `--reset` drops the database,
    // and empty, damaged or refused input must not cost the data.
    let read = dump_file::DumpRead {
        gzip_flag: options.gzip,
        gz_name: false,
        origin: options.origin,
        shared_postgres: shared_postgres(service),
        postgres: service.driver == crate::config::Driver::Postgres,
        // As the restore client names it (`piped_client_args`).
        database: service.database.as_deref().unwrap_or("app"),
    };
    let reader = stdin_spool::spool_stdin(std::io::stdin().lock(), read)?;
    crate::docker::ensure_client_reachable(service, options.reset)?;
    prepare_for_restore(service, options.reset)?;

    from_stdin::restore_from_stdin(service, reader, options)
}

/// Checks the dump at `file_path` as a restore of it would, changing
/// nothing.
pub(crate) fn check_dump(
    service: &crate::config::ServiceConfig,
    file_path: &Path,
    options: RestoreOptions,
) -> Result<()> {
    let read = dump_file::DumpRead {
        gzip_flag: options.gzip,
        gz_name: from_file::is_gzip_path(file_path),
        origin: options.origin,
        shared_postgres: shared_postgres(service),
        postgres: service.driver == crate::config::Driver::Postgres,
        // As the restore client names it (`piped_client_args`).
        database: service.database.as_deref().unwrap_or("app"),
    };
    dump_file::check_dump_file(file_path, read).map(drop)
}

/// Whether the database already holds tables (a seed goes only into an
/// empty one).
pub(crate) fn has_tables(service: &crate::config::ServiceConfig) -> Result<bool> {
    has_tables::has_tables(service)
}

/// A shared server's PostgreSQL tenant (see `DumpRead::shared_postgres`).
fn shared_postgres(service: &crate::config::ServiceConfig) -> bool {
    service.runtime.shared.is_some() && service.driver == crate::config::Driver::Postgres
}

fn prepare_for_restore(service: &crate::config::ServiceConfig, reset: bool) -> Result<()> {
    prepare_restore(service, reset)
}
