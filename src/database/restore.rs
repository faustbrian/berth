//! database restore module.
//!
//! Contains database restore logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

use prepare::prepare_restore;

mod from_file;
mod from_stdin;
mod prepare;
mod process;

pub(crate) fn restore(
    service: &crate::config::ServiceConfig,
    file_path: &Path,
    reset: bool,
    gzip: bool,
) -> Result<()> {
    prepare_for_restore(service, reset)?;

    from_file::restore_from_file(service, file_path, gzip)
}

pub(crate) fn restore_stdin(
    service: &crate::config::ServiceConfig,
    reset: bool,
    gzip: bool,
) -> Result<()> {
    prepare_for_restore(service, reset)?;

    from_stdin::restore_from_stdin(service, gzip)
}

fn prepare_for_restore(service: &crate::config::ServiceConfig, reset: bool) -> Result<()> {
    prepare_restore(service, reset)
}
