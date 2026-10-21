//! SQL database setup and restore operations.

#![allow(clippy::print_stdout)] // Operations need to print status

use anyhow::Result;
use std::path::Path;

use crate::config::ServiceConfig;

mod dump;
mod post_restore;
mod restore;
mod setup;
mod sql_admin;

pub(crate) use post_restore::PostRestoreOptions;

pub fn setup(service: &ServiceConfig, timeout: Option<u64>) -> Result<()> {
    setup::setup(service, timeout)
}

pub(crate) use restore::{DumpOrigin, RestoreOptions};

pub(crate) fn restore(
    service: &ServiceConfig,
    file_path: &Path,
    options: RestoreOptions,
) -> Result<()> {
    restore::restore(service, file_path, options)
}

pub fn dump(service: &ServiceConfig, file_path: &Path, gzip: bool) -> Result<()> {
    dump::dump(service, file_path, gzip)
}

pub fn dump_stdout(service: &ServiceConfig, gzip: bool) -> Result<()> {
    dump::dump_stdout(service, gzip)
}

pub(crate) fn check_dump(
    service: &ServiceConfig,
    file_path: &Path,
    options: RestoreOptions,
) -> Result<()> {
    restore::check_dump(service, file_path, options)
}

pub(crate) fn has_tables(service: &ServiceConfig) -> Result<bool> {
    restore::has_tables(service)
}

pub(crate) fn restore_stdin(service: &ServiceConfig, options: RestoreOptions) -> Result<()> {
    restore::restore_stdin(service, options)
}

/// Runs optional post-restore app hooks (`migrate` and/or `schema:dump`).
pub fn run_laravel_post_restore(options: PostRestoreOptions<'_>) -> Result<()> {
    post_restore::run_laravel_post_restore(options)
}
