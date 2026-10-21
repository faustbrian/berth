//! Streams a dump program's output to its destination without holding it.

use anyhow::{Context, Result};
use std::io::{Read, Write};

/// How much of the dump program's stderr is kept for a failure message.
const KEPT_STDERR: usize = 64 * 1024;

/// Runs `docker args` (a dump to stdout) and copies its stdout into `sink`
/// as it comes, its stderr read alongside so neither pipe fills. Returns the
/// bytes copied. Fails when the dump program does, with its stderr.
pub(super) fn stream_dump(args: &[String], sink: &mut dyn Write, failure: &str) -> Result<u64> {
    let refs = crate::docker::docker_arg_refs(args);
    let mut child = crate::docker::spawn_docker_stdout_stderr_piped(
        &refs,
        &crate::docker::runtime_command_error_context("dump"),
    )?;
    let mut stdout = child
        .stdout
        .take()
        .context("Failed to open the dump's stdout")?;
    let stderr = child
        .stderr
        .take()
        .context("Failed to open the dump's stderr")?;
    // Kept up to a limit, the rest read and dropped: a closed pipe would
    // kill the dump on its next warning.
    let reader = std::thread::spawn(move || {
        let mut stderr = stderr;
        let mut kept = Vec::new();
        drop(
            (&mut stderr)
                .take(KEPT_STDERR as u64)
                .read_to_end(&mut kept),
        );
        drop(std::io::copy(&mut stderr, &mut std::io::sink()));
        kept
    });
    let copied = std::io::copy(&mut stdout, sink);
    if copied.is_err() {
        // The destination failed (a full disk): stop the dump too.
        drop(child.kill());
    }
    drop(stdout);
    let status = child.wait().context("Failed to wait for the dump")?;
    let stderr = reader
        .join()
        .map_err(|_| anyhow::anyhow!("the dump's stderr reader panicked"))?;
    let copied = copied.context("Failed to write the dump")?;
    let output = std::process::Output {
        status,
        stdout: Vec::new(),
        stderr,
    };
    super::super::sql_admin::ensure_sql_command_success(&output, failure)?;
    Ok(copied)
}
