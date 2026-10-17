//! cli support tail access logs module.
//!
//! Contains cli support tail access logs logic used by Berth command workflows.

use anyhow::{Context, Result};
use std::process::Command;

use crate::output::{self, LogLevel, Persistence};
use crate::{docker, serve};

pub(crate) fn tail_access_logs(follow: bool, tail: Option<u64>) -> Result<()> {
    let path = serve::caddy_access_log_path()?;
    if !path.exists() {
        anyhow::bail!(
            "access log file not found at {}. start an app via `berth up` first",
            path.display()
        );
    }

    let mut args = vec!["-n".to_owned(), tail.unwrap_or(100).to_string()];
    if follow {
        args.push("-f".to_owned());
    }
    args.push(path.display().to_string());

    if docker::is_dry_run() {
        output::event(
            "logs",
            LogLevel::Info,
            &format!("[dry-run] tail {}", args.join(" ")),
            Persistence::Transient,
        );
        return Ok(());
    }

    let status = Command::new("tail")
        .args(args.iter().map(String::as_str))
        .status()
        .context("failed to tail access log file")?;
    if status.success() {
        return Ok(());
    }
    anyhow::bail!("tail command failed")
}

#[cfg(test)]
mod tests {
    use super::tail_access_logs;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    static TAIL_ACCESS_LOGS_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn with_log_lock<R>(test: impl FnOnce(&PathBuf) -> R) -> R {
        let guard = TAIL_ACCESS_LOGS_LOCK.get_or_init(Default::default).lock();
        let guard = match guard {
            Ok(guard) => guard,
            Err(err) => err.into_inner(),
        };

        let path = temp_log_path();
        let result = test(&path);
        cleanup_state_dir(&path);
        drop(guard);
        result
    }

    fn temp_log_path() -> PathBuf {
        let path = crate::serve::caddy_access_log_path().expect("access log path");
        let state_dir = path.parent().expect("access log parent").to_path_buf();
        drop(fs::remove_dir_all(&state_dir));
        fs::create_dir_all(&state_dir).expect("create caddy dir");
        path
    }

    fn cleanup_state_dir(path: &PathBuf) {
        if let Some(state_dir) = path.parent() {
            drop(fs::remove_dir_all(state_dir));
        }
    }

    #[test]
    fn tail_access_logs_requires_existing_log_file() {
        let result = with_log_lock(|path| {
            drop(fs::remove_file(path));
            tail_access_logs(false, None)
        });
        assert!(result.is_err());
        assert!(
            result
                .expect_err("missing log")
                .to_string()
                .contains("access log file")
        );
    }

    #[test]
    fn tail_access_logs_dry_run_reports() {
        let result = with_log_lock(|path| {
            fs::write(path, "line\n").expect("create log file");
            crate::docker::with_dry_run_lock(|| tail_access_logs(true, Some(25)))
        });

        assert!(result.is_ok());
    }

    #[test]
    fn tail_access_logs_executes_tail_binary() {
        let result = with_log_lock(|path| {
            fs::write(path, "line\n").expect("create log file");
            tail_access_logs(false, Some(5))
        });
        assert!(result.is_ok());
    }
}
