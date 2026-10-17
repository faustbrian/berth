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

/// Waits for for restore success to reach a ready state.
pub(super) fn wait_for_restore_success(child: Child) -> Result<()> {
    let output = child
        .wait_with_output()
        .context("Failed to wait for restore process")?;

    super::super::sql_admin::ensure_sql_command_success(&output, "Database restore failed")
}

#[cfg(test)]
mod tests {
    use super::wait_for_restore_success;
    use std::process::{Command, Stdio};

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
