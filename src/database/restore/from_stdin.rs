//! database restore from stdin module.
//!
//! Contains database restore from stdin logic used by Berth command workflows.

use anyhow::{Context, Result};
use std::io::Read;

use crate::config::ServiceConfig;

use super::RestoreOptions;
use super::process::{finish_restore, start_restore_process};

/// Streams the opened dump (see `dump_file::open_dump`) into the database.
pub(super) fn restore_from_stdin(
    service: &ServiceConfig,
    reader: Box<dyn Read>,
    options: RestoreOptions,
) -> Result<()> {
    let (mut reader, statements) = super::process::tap(service, reader);
    let mut restore = start_restore_process(service)?;
    let streamed = std::io::copy(&mut reader, &mut restore.stdin)
        .map(drop)
        .context("Failed to pipe stdin to database process")
        .and_then(|()| restore.send_end());
    finish_restore(service, options, restore, streamed, &statements)
}
