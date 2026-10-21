//! Persisted daemon session metadata.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
use std::cell::RefCell;

#[cfg(test)]
thread_local! {
    static TEST_DAEMON_HOME: RefCell<Option<String>> = const { RefCell::new(None) };
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DaemonSession {
    pub(crate) project_root: String,
    pub(crate) pid: u32,
    pub(crate) log_path: String,
    pub(crate) started_at_unix: u64,
}

#[cfg(test)]
pub(crate) fn set_test_daemon_home(home: &str) {
    TEST_DAEMON_HOME.with(|value| *value.borrow_mut() = Some(home.to_owned()));
}

#[cfg(test)]
pub(crate) fn clear_test_daemon_home() {
    TEST_DAEMON_HOME.with(|value| *value.borrow_mut() = None);
}

#[cfg(test)]
fn test_daemon_home() -> Option<String> {
    TEST_DAEMON_HOME.with(|value| value.borrow().clone())
}

pub(crate) fn load_session(project_root: &Path) -> Result<Option<DaemonSession>> {
    let session_path = session_path(project_root)?;
    if !session_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&session_path)
        .with_context(|| format!("failed to read {}", session_path.display()))?;
    toml::from_str(&content)
        .with_context(|| format!("failed to parse {}", session_path.display()))
        .map(Some)
}

pub(crate) fn save_session(project_root: &Path, session: &DaemonSession) -> Result<()> {
    let session_path = session_path(project_root)?;
    if let Some(parent) = session_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let content = toml::to_string_pretty(session).context("failed to serialize daemon session")?;
    fs::write(&session_path, content)
        .with_context(|| format!("failed to write {}", session_path.display()))
}

pub(crate) fn clear_session(project_root: &Path) -> Result<()> {
    let session_path = session_path(project_root)?;
    if session_path.exists() {
        fs::remove_file(&session_path)
            .with_context(|| format!("failed to remove {}", session_path.display()))?;
    }
    Ok(())
}

pub(crate) fn daemon_log_path(project_root: &Path) -> Result<PathBuf> {
    let project_dir = project_state_dir(project_root)?;
    Ok(project_dir.join("daemon.log"))
}

pub(crate) fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn session_path(project_root: &Path) -> Result<PathBuf> {
    let project_dir = project_state_dir(project_root)?;
    Ok(project_dir.join("session.toml"))
}

fn project_state_dir(project_root: &Path) -> Result<PathBuf> {
    let home = daemon_home_dir()?;
    Ok(home.join("projects").join(project_key(project_root)))
}

fn daemon_home_dir() -> Result<PathBuf> {
    #[cfg(test)]
    if let Some(home) = test_daemon_home() {
        return Ok(PathBuf::from(home).join(".config/berth/daemon"));
    }

    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".config/berth/daemon"))
}

fn project_key(project_root: &Path) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in project_root.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3_u64);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::{DaemonSession, clear_session, daemon_log_path, load_session, save_session};
    use std::fs;

    fn temp_home(name: &str) -> std::path::PathBuf {
        let home = crate::test_support::temp_root().join(format!(
            "berth-daemon-state-{name}-{}",
            crate::test_support::unique_stamp()
        ));
        drop(fs::remove_dir_all(&home));
        fs::create_dir_all(&home).expect("create temp home");
        home
    }

    fn temp_project_root(name: &str) -> std::path::PathBuf {
        let root = crate::test_support::temp_root().join(format!(
            "berth-daemon-project-{name}-{}",
            crate::test_support::unique_stamp()
        ));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(root.join("nested")).expect("create temp root");
        root.join("nested")
    }

    #[test]
    fn session_round_trip_uses_project_keyed_paths() {
        let home = temp_home("round-trip");
        super::set_test_daemon_home(home.to_str().expect("home path"));
        let project_root = temp_project_root("round-trip");

        let session = DaemonSession {
            project_root: project_root.to_string_lossy().into_owned(),
            pid: 1234,
            log_path: daemon_log_path(&project_root)
                .expect("log path")
                .to_string_lossy()
                .into_owned(),
            started_at_unix: 42,
        };

        save_session(&project_root, &session).expect("save session");
        let loaded = load_session(&project_root)
            .expect("load session")
            .expect("session should exist");
        assert_eq!(loaded.pid, 1234);
        assert_eq!(loaded.project_root, session.project_root);

        clear_session(&project_root).expect("clear session");
        assert!(load_session(&project_root).expect("load cleared").is_none());
        super::clear_test_daemon_home();
    }
}
