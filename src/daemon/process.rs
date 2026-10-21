//! Detached daemon process helpers.

use anyhow::{Context, Result};
use std::fs::OpenOptions;
use std::path::Path;
use std::process::{Command, Stdio};

#[cfg(test)]
use std::cell::RefCell;

#[cfg(test)]
thread_local! {
    static TEST_DAEMON_BINARY: RefCell<Option<String>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn set_test_daemon_binary(path: &str) {
    TEST_DAEMON_BINARY.with(|value| *value.borrow_mut() = Some(path.to_owned()));
}

#[cfg(test)]
pub(crate) fn clear_test_daemon_binary() {
    TEST_DAEMON_BINARY.with(|value| *value.borrow_mut() = None);
}

#[cfg(test)]
fn test_daemon_binary() -> Option<String> {
    TEST_DAEMON_BINARY.with(|value| value.borrow().clone())
}

pub(crate) fn daemon_binary() -> Result<String> {
    #[cfg(test)]
    if let Some(path) = test_daemon_binary() {
        return Ok(path);
    }

    std::env::current_exe()
        .context("failed to resolve current berth executable")
        .map(|path| path.to_string_lossy().into_owned())
}

pub(crate) fn spawn_detached(project_root: &Path, log_path: &Path) -> Result<u32> {
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .with_context(|| format!("failed to open {}", log_path.display()))?;
    let stderr_log = log
        .try_clone()
        .with_context(|| format!("failed to clone {}", log_path.display()))?;

    let child = Command::new(daemon_binary()?)
        .arg("daemon")
        .arg("run")
        .arg("--path")
        .arg(project_root)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr_log))
        .spawn()
        .with_context(|| format!("failed to start daemon for {}", project_root.display()))?;

    Ok(child.id())
}

pub(crate) fn pid_is_running(pid: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .is_ok_and(|status| status.success())
}

pub(crate) fn stop_pid(pid: u32) -> Result<()> {
    if !pid_is_running(pid) {
        return Ok(());
    }

    let status = Command::new("kill")
        .arg(pid.to_string())
        .status()
        .context("failed to stop daemon process")?;

    if !status.success() {
        anyhow::bail!("failed to stop daemon process pid {pid}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    fn temp_log_path(name: &str) -> std::path::PathBuf {
        crate::test_support::temp_root().join(format!(
            "berth-daemon-log-{name}-{}",
            crate::test_support::unique_stamp()
        ))
    }

    fn mock_binary(name: &str, script: &str) -> String {
        let root = crate::test_support::temp_root().join(format!(
            "berth-daemon-binary-{name}-{}",
            crate::test_support::unique_stamp()
        ));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).expect("create temp root");
        let path = root.join("berth");
        fs::write(&path, script).expect("write mock daemon");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&path).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&path, perms).expect("chmod");
        }
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn spawn_detached_starts_mock_binary() {
        let binary = mock_binary("spawn", "#!/usr/bin/env sh\nsleep 60\n");
        super::set_test_daemon_binary(&binary);

        let log_path = temp_log_path("spawn");
        let pid =
            super::spawn_detached(Path::new("/tmp/project"), &log_path).expect("spawn detached");

        assert!(pid > 0);
        assert!(super::pid_is_running(pid));

        super::stop_pid(pid).expect("stop daemon pid");
        super::clear_test_daemon_binary();
    }
}
