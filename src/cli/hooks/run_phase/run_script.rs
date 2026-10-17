//! hook script execution helpers.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::output::{self, LogLevel, Persistence};

pub(super) fn run_script_hook(
    path: &str,
    timeout_sec: Option<u64>,
    workspace_root: &Path,
) -> Result<()> {
    let resolved = resolve_script_path(path, workspace_root);
    let command_display = format!("sh {}", resolved.display());

    if crate::docker::is_dry_run() {
        output::event(
            "hooks",
            LogLevel::Info,
            &format!("[dry-run] {command_display}"),
            Persistence::Transient,
        );
        return Ok(());
    }

    let mut child = Command::new("sh")
        .arg(&resolved)
        .current_dir(workspace_root)
        .spawn()
        .with_context(|| format!("failed to execute hook script at {}", resolved.display()))?;

    if let Some(timeout) = timeout_sec {
        wait_with_timeout(&mut child, Duration::from_secs(timeout), &command_display)?;
    } else {
        let status = child
            .wait()
            .with_context(|| format!("failed waiting for hook script {}", resolved.display()))?;
        if !status.success() {
            anyhow::bail!(
                "hook script exited with non-zero status: {}",
                resolved.display()
            );
        }
    }

    Ok(())
}

fn wait_with_timeout(
    child: &mut std::process::Child,
    timeout: Duration,
    command_display: &str,
) -> Result<()> {
    let started = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .context("failed while polling hook script")?
        {
            if status.success() {
                return Ok(());
            }
            anyhow::bail!("hook command exited with non-zero status: {command_display}");
        }

        if started.elapsed() > timeout {
            child
                .kill()
                .context("failed to terminate timed-out hook script")?;
            drop(child.wait());
            anyhow::bail!(
                "hook command timed out after {}s: {command_display}",
                timeout.as_secs()
            );
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}

pub(super) fn resolve_script_path(path: &str, workspace_root: &Path) -> PathBuf {
    crate::cli::support::resolve_workspace_path(workspace_root, path)
}

#[cfg(test)]
mod tests {
    use super::{resolve_script_path, run_script_hook};
    use std::env;
    use std::fs;
    use std::path::Path;

    fn write_script(path: &Path, body: &str) -> String {
        let script_path = path.to_path_buf();
        fs::write(&script_path, format!("#!/bin/sh\n{body}")).expect("write script file");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script_path)
                .expect("read script metadata")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script_path, perms).expect("set script permissions");
        }

        script_path.to_string_lossy().to_string()
    }

    #[test]
    fn run_script_hook_executes_resolved_script() {
        let dir = env::temp_dir().join(format!("berth-run-script-{}", std::process::id()));
        drop(fs::remove_dir_all(&dir));
        fs::create_dir_all(&dir).expect("create temp dir");
        let script = write_script(&dir.join("run.sh"), "printf '%s' ok; exit 0");

        crate::docker::with_dry_run_state(false, || {
            run_script_hook(&script, None, &dir).expect("run script hook");
        });
    }

    #[test]
    fn run_script_hook_reports_non_zero_exit() {
        let dir = env::temp_dir().join(format!("berth-run-script-{}", std::process::id() + 1));
        drop(fs::remove_dir_all(&dir));
        fs::create_dir_all(&dir).expect("create temp dir");
        let script = write_script(&dir.join("fail.sh"), "echo boom; exit 3");

        let error = crate::docker::with_dry_run_state(false, || {
            run_script_hook(&script, None, &dir).expect_err("expected script error path")
        });
        assert!(error.to_string().contains("non-zero status"));
    }

    #[test]
    fn run_script_hook_times_out_and_kills_slow_script() {
        let dir = env::temp_dir().join(format!("berth-run-script-{}", std::process::id() + 2));
        drop(fs::remove_dir_all(&dir));
        fs::create_dir_all(&dir).expect("create temp dir");
        let script = write_script(&dir.join("sleep.sh"), "trap 'exit 0' TERM INT; sleep 5");

        let error = crate::docker::with_dry_run_state(false, || {
            run_script_hook(&script, Some(1), &dir).expect_err("expected timeout")
        });
        assert!(error.to_string().contains("timed out"));
    }

    #[test]
    fn resolve_script_path_joins_workspace_root_for_relative_paths() {
        let root = Path::new("/tmp/berth-workspace");
        let resolved = resolve_script_path(".berth/hooks/hook.sh", root);
        assert_eq!(resolved, root.join(".berth/hooks/hook.sh"));
    }

    #[test]
    fn run_script_hook_dry_run_skips_execution() {
        let result = crate::docker::with_dry_run_lock(|| {
            let dir =
                env::temp_dir().join(format!("berth-run-script-dry-run-{}", std::process::id()));
            drop(fs::remove_dir_all(&dir));
            fs::create_dir_all(&dir).expect("create temp dir");

            let result = run_script_hook(
                &dir.join("missing.sh").display().to_string(),
                None,
                Path::new("/tmp"),
            );
            drop(fs::remove_dir_all(&dir));
            result
        });

        assert!(result.is_ok());
    }
}
