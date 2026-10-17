//! database restore from stdin module.
//!
//! Contains database restore from stdin logic used by Berth command workflows.

use anyhow::{Context, Result};
use flate2::read::GzDecoder;

use crate::config::ServiceConfig;

use super::process::{start_restore_process, wait_for_restore_success};

pub(super) fn restore_from_stdin(service: &ServiceConfig, gzip: bool) -> Result<()> {
    let mut restore = start_restore_process(service)?;

    if gzip {
        let stdin = std::io::stdin();
        let mut decoder = GzDecoder::new(stdin.lock());
        std::io::copy(&mut decoder, &mut restore.stdin)
            .context("Failed to pipe gzip stdin to database process")?;
    } else {
        std::io::copy(&mut std::io::stdin().lock(), &mut restore.stdin)
            .context("Failed to pipe stdin to database process")?;
    }

    drop(restore.stdin);
    wait_for_restore_success(restore.child)
}
