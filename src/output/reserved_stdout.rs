//! Keeps stdout for one machine-readable document.
//!
//! `up --json` prints exactly one JSON value on stdout. Every child process
//! berth runs for its own work in that command (image pulls and builds,
//! bootstrap and seed commands, swarm dependency output) then writes to
//! stderr instead, so nothing can precede or follow the document.

use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

static RESERVED: AtomicBool = AtomicBool::new(false);

/// Reserves stdout for the command's machine-readable result.
pub(crate) fn reserve_stdout() {
    RESERVED.store(true, Ordering::Relaxed);
}

/// Whether stdout is reserved for a machine-readable result.
#[must_use]
pub(crate) fn stdout_reserved() -> bool {
    RESERVED.load(Ordering::Relaxed)
}

/// The stdout a child process should get: berth's stderr while stdout is
/// reserved, else the inherited stdout.
#[must_use]
pub(crate) fn child_stdout() -> Stdio {
    if stdout_reserved() {
        Stdio::from(std::io::stderr())
    } else {
        Stdio::inherit()
    }
}
