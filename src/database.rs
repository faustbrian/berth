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

pub fn restore(service: &ServiceConfig, file_path: &Path, reset: bool, gzip: bool) -> Result<()> {
    restore::restore(service, file_path, reset, gzip)
}

pub fn dump(service: &ServiceConfig, file_path: &Path, gzip: bool) -> Result<()> {
    dump::dump(service, file_path, gzip)
}

pub fn dump_stdout(service: &ServiceConfig, gzip: bool) -> Result<()> {
    dump::dump_stdout(service, gzip)
}

pub fn restore_stdin(service: &ServiceConfig, reset: bool, gzip: bool) -> Result<()> {
    restore::restore_stdin(service, reset, gzip)
}

/// Runs optional post-restore app hooks (`migrate` and/or `schema:dump`).
pub fn run_laravel_post_restore(options: PostRestoreOptions<'_>) -> Result<()> {
    post_restore::run_laravel_post_restore(options)
}
