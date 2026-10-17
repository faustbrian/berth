//! database restore process module.
//!
//! Contains database restore process logic used by Berth command workflows.

use anyhow::{Context, Result};
use std::process::{Child, ChildStdin};

use crate::config::ServiceConfig;

pub(super) struct RestoreProcess {
    pub(super) child: Child,
    pub(super) stdin: ChildStdin,
}

pub(super) fn start_restore_process(service: &ServiceConfig) -> Result<RestoreProcess> {
    let mut child =
        crate::docker::exec_piped(service, false).context("Failed to start restore process")?;
    let stdin = child.stdin.take().context("Failed to open stdin pipe")?;

    Ok(RestoreProcess { child, stdin })
}

/// Ends a restore after streaming: the database's own result wins over a
/// write error. A client that rejects the dump exits and closes its stdin, so
/// berth's next write fails with a broken pipe; reporting that would hide
/// the SQL error that caused it (ledger L117, FI-03).
///
/// # Errors
///
/// Returns the database process's failure, else the streaming error.
pub(super) fn finish_restore(child: Child, stdin: ChildStdin, streamed: Result<()>) -> Result<()> {
    drop(stdin);
    wait_for_restore_success(child)?;
    streamed
}

/// Waits for for restore success to reach a ready state.
pub(super) fn wait_for_restore_success(child: Child) -> Result<()> {
    let output = child
        .wait_with_output()
        .context("Failed to wait for restore process")?;

    super::super::sql_admin::ensure_sql_command_success(&output, "Database restore failed")
}

#[cfg(test)]
mod tests {
    use super::{finish_restore, wait_for_restore_success};
    use std::io::Write;
    use std::process::{Command, Stdio};

    #[test]
    fn a_rejected_dump_reports_the_database_error_not_the_broken_pipe() {
        // The "client" reads one line, rejects it and exits; later writes
        // hit a closed pipe.
        let mut child = Command::new("sh")
            .args([
                "-c",
                "read line; echo 'ERROR 1064: syntax error near FOO' >&2; exit 1",
            ])
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn client");
        let mut stdin = child.stdin.take().expect("stdin");
        let streamed = (|| -> anyhow::Result<()> {
            for _ in 0..100_000 {
                stdin.write_all(b"FOO BAR;\n")?;
            }
            Ok(())
        })();
        assert!(streamed.is_err(), "the pipe closed");
        let error = finish_restore(child, stdin, streamed).expect_err("restore fails");
        assert!(
            error.to_string().contains("syntax error near FOO"),
            "{error:#}"
        );
    }

    #[test]
    fn a_streaming_error_stands_when_the_database_succeeded() {
        let mut child = Command::new("sh")
            .args(["-c", "cat >/dev/null"])
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn client");
        let stdin = child.stdin.take().expect("stdin");
        let error = finish_restore(
            child,
            stdin,
            Err(anyhow::anyhow!("Failed to read from SQL dump file")),
        )
        .expect_err("read error");
        assert!(error.to_string().contains("Failed to read"), "{error:#}");
    }

    #[test]
    fn wait_for_restore_success_reports_non_utf8_stderr() {
        let child = Command::new("sh")
            .args(["-c", "printf '\\377' >&2; exit 1"])
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn failing command");

        let error = wait_for_restore_success(child).expect_err("expected restore failure");
        let rendered = error.to_string();

        assert!(rendered.starts_with("Database restore failed:"));
        assert_ne!(rendered.trim_end(), "Database restore failed:");
    }
}
