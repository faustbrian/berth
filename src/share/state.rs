//! Persistent state and process lifecycle for sharing sessions.

use anyhow::{Context, Result};
use std::fs::OpenOptions;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::config::ServiceConfig;
use crate::share::{ShareProvider, ShareSession, ShareSessionStatus, ShareStartResult};

#[cfg(test)]
use std::cell::RefCell;

#[cfg(test)]
thread_local! {
    static TEST_STATE_HOME: RefCell<Option<String>> = const { RefCell::new(None) };
    static TEST_PROVIDER_BINARIES: RefCell<Vec<(ShareProvider, String)>> =
        const { RefCell::new(Vec::new()) };
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct ShareState {
    sessions: Vec<ShareSession>,
}

#[cfg(test)]
fn set_test_state_home(home: &str) {
    TEST_STATE_HOME.with(|value| *value.borrow_mut() = Some(home.to_owned()));
}

#[cfg(test)]
fn clear_test_state_home() {
    TEST_STATE_HOME.with(|value| *value.borrow_mut() = None);
}

#[cfg(test)]
fn set_test_provider_binary(provider: ShareProvider, path: &str) {
    TEST_PROVIDER_BINARIES.with(|value| {
        let mut binaries = value.borrow_mut();
        if let Some((_, binary)) = binaries.iter_mut().find(|(item, _)| *item == provider) {
            *binary = path.to_owned();
            return;
        }
        binaries.push((provider, path.to_owned()));
    });
}

#[cfg(test)]
fn clear_test_provider_binaries() {
    TEST_PROVIDER_BINARIES.with(|value| value.borrow_mut().clear());
}

#[cfg(test)]
fn test_state_home() -> Option<String> {
    TEST_STATE_HOME.with(|value| value.borrow().clone())
}

#[cfg(test)]
fn test_provider_binary(provider: ShareProvider) -> Option<String> {
    TEST_PROVIDER_BINARIES.with(|value| {
        value
            .borrow()
            .iter()
            .find(|(item, _)| *item == provider)
            .map(|(_, binary)| binary.clone())
    })
}

fn provider_binary(provider: ShareProvider) -> String {
    #[cfg(test)]
    if let Some(custom) = test_provider_binary(provider) {
        return custom;
    }
    provider.command_binary().to_owned()
}

pub(super) fn start_session(
    target: &ServiceConfig,
    provider: ShareProvider,
    detached: bool,
    timeout_secs: u64,
) -> Result<ShareStartResult> {
    ensure_provider_available(provider)?;

    let local_url = super::target::local_target_url(target);
    let command_args = provider.command_args(&local_url);
    let started_at_unix = now_unix();
    let session_id = session_id(provider, &target.name);
    let verify_timeout = Duration::from_secs(timeout_secs.max(1));

    let paths = state_paths()?;
    std::fs::create_dir_all(&paths.logs_dir)
        .with_context(|| format!("failed to create {}", paths.logs_dir.display()))?;

    let log_path = paths.logs_dir.join(format!("{session_id}.log"));
    if provider == ShareProvider::Tailscale {
        return start_tailscale_bg_session(
            target,
            provider,
            &local_url,
            &command_args,
            started_at_unix,
            &session_id,
            &paths.state_path,
            &log_path,
            verify_timeout,
        );
    }

    let status = if detached {
        let pid = spawn_detached(provider, &command_args, &log_path)?;
        let public_url = wait_for_public_url(&log_path, provider, verify_timeout);
        let mut session = ShareSession {
            id: session_id,
            provider,
            service: target.name.clone(),
            local_url,
            public_url,
            pid: Some(pid),
            log_path: log_path.to_string_lossy().to_string(),
            command: command_for_state(provider, &command_args),
            started_at_unix,
        };
        if let Err(err) = verify_public_session(&mut session, verify_timeout) {
            stop_pid(pid)?;
            return Err(err);
        }
        upsert_session(&paths.state_path, session.clone())?;
        session.to_status(true)
    } else {
        let mut child = spawn_foreground(provider, &command_args)?;
        let session = ShareSession {
            id: session_id,
            provider,
            service: target.name.clone(),
            local_url,
            public_url: None,
            pid: Some(child.id()),
            log_path: log_path.to_string_lossy().to_string(),
            command: command_for_state(provider, &command_args),
            started_at_unix,
        };
        upsert_session(&paths.state_path, session.clone())?;
        let exit = child
            .wait()
            .context("failed waiting for share provider command")?;
        remove_session(&paths.state_path, &session.id)?;

        let mut status = session.to_status(exit.success());
        status.running = false;
        if let Some(code) = exit.code() {
            return Ok(ShareStartResult {
                session: status,
                foreground_exit_code: Some(code),
            });
        }
        return Ok(ShareStartResult {
            session: status,
            foreground_exit_code: Some(1),
        });
    };

    Ok(ShareStartResult {
        session: status,
        foreground_exit_code: None,
    })
}

pub(super) fn status_sessions(
    service: Option<&str>,
    provider: Option<ShareProvider>,
) -> Result<Vec<ShareSessionStatus>> {
    let paths = state_paths()?;
    let mut state = read_state(&paths.state_path)?;
    hydrate_public_urls(&mut state.sessions);

    let statuses = state
        .sessions
        .iter()
        .filter(|session| matches_filter(session, service, provider))
        .map(|session| session.to_status(session_running_for(session)))
        .collect();
    write_state(&paths.state_path, &state)?;
    Ok(statuses)
}

pub(super) fn stop_sessions(
    service: Option<&str>,
    provider: Option<ShareProvider>,
    all: bool,
) -> Result<Vec<ShareSessionStatus>> {
    let paths = state_paths()?;
    let mut state = read_state(&paths.state_path)?;

    let mut removed = Vec::new();
    let mut retained = Vec::new();
    let mut stop_tailscale = false;
    for session in state.sessions {
        let should_stop = all || matches_filter(&session, service, provider);
        if !should_stop {
            retained.push(session);
            continue;
        }

        if session.provider == ShareProvider::Tailscale {
            stop_tailscale = true;
        } else if let Some(pid) = session.pid {
            stop_pid(pid)?;
        }

        removed.push(session);
    }
    if stop_tailscale {
        stop_tailscale_funnel()?;
    }
    state.sessions = retained;

    write_state(&paths.state_path, &state)?;
    Ok(removed
        .into_iter()
        .map(|session| session.to_status(false))
        .collect())
}

struct ShareStatePaths {
    state_path: PathBuf,
    logs_dir: PathBuf,
}

fn state_paths() -> Result<ShareStatePaths> {
    #[cfg(test)]
    if let Some(home) = test_state_home() {
        return Ok(state_paths_with_home(&home));
    }
    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(state_paths_with_home(&home))
}

fn state_paths_with_home(home: &str) -> ShareStatePaths {
    let share_dir = PathBuf::from(home).join(".config/berth/share");
    ShareStatePaths {
        state_path: share_dir.join("sessions.toml"),
        logs_dir: share_dir.join("logs"),
    }
}

fn read_state(path: &Path) -> Result<ShareState> {
    if !path.exists() {
        return Ok(ShareState::default());
    }

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    toml::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))
}

fn write_state(path: &Path, state: &ShareState) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let serialized = toml::to_string_pretty(state).context("failed to serialize share state")?;
    std::fs::write(path, serialized).with_context(|| format!("failed to write {}", path.display()))
}

fn upsert_session(path: &Path, session: ShareSession) -> Result<()> {
    let mut state = read_state(path)?;
    if let Some(existing) = state.sessions.iter_mut().find(|item| item.id == session.id) {
        *existing = session;
    } else {
        state.sessions.push(session);
    }
    write_state(path, &state)
}

fn remove_session(path: &Path, id: &str) -> Result<()> {
    let mut state = read_state(path)?;
    state.sessions.retain(|session| session.id != id);
    write_state(path, &state)
}

fn session_id(provider: ShareProvider, service: &str) -> String {
    format!("{provider}-{service}")
}

fn command_for_state(provider: ShareProvider, args: &[String]) -> Vec<String> {
    let mut command = vec![provider.command_binary().to_owned()];
    command.extend(args.iter().cloned());
    command
}

fn spawn_detached(provider: ShareProvider, args: &[String], log_path: &Path) -> Result<u32> {
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .with_context(|| format!("failed to open {}", log_path.display()))?;

    let stderr_log = log
        .try_clone()
        .with_context(|| format!("failed to clone {}", log_path.display()))?;

    let child = Command::new(provider_binary(provider))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr_log))
        .spawn()
        .with_context(|| format!("failed to start {}", provider_binary(provider)))?;

    Ok(child.id())
}

fn spawn_foreground(provider: ShareProvider, args: &[String]) -> Result<std::process::Child> {
    Command::new(provider_binary(provider))
        .args(args)
        .spawn()
        .with_context(|| format!("failed to start {}", provider_binary(provider)))
}

fn ensure_provider_available(provider: ShareProvider) -> Result<()> {
    match Command::new(provider_binary(provider))
        .arg("--version")
        .output()
    {
        Ok(_) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!(
                "required provider binary '{}' was not found on PATH",
                provider_binary(provider)
            )
        }
        Err(err) => {
            Err(err).with_context(|| format!("failed to invoke {}", provider_binary(provider)))
        }
    }
}

fn start_tailscale_bg_session(
    target: &ServiceConfig,
    provider: ShareProvider,
    local_url: &str,
    command_args: &[String],
    started_at_unix: u64,
    session_id: &str,
    state_path: &Path,
    log_path: &Path,
    verify_timeout: Duration,
) -> Result<ShareStartResult> {
    let output = Command::new(provider_binary(provider))
        .args(command_args)
        .output()
        .with_context(|| format!("failed to start {}", provider_binary(provider)))?;
    append_command_output(log_path, &output)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        anyhow::bail!(
            "tailscale funnel failed: {}",
            if stderr.is_empty() {
                "unknown error".to_owned()
            } else {
                stderr
            }
        );
    }

    let public_url = extract_provider_url(&String::from_utf8_lossy(&output.stdout), provider)
        .or_else(tailscale_public_url_from_status);
    let mut session = ShareSession {
        id: session_id.to_owned(),
        provider,
        service: target.name.clone(),
        local_url: local_url.to_owned(),
        public_url,
        pid: None,
        log_path: log_path.to_string_lossy().to_string(),
        command: command_for_state(provider, command_args),
        started_at_unix,
    };
    verify_public_session(&mut session, verify_timeout)?;
    upsert_session(state_path, session.clone())?;
    Ok(ShareStartResult {
        session: session.to_status(session_running_for(&session)),
        foreground_exit_code: None,
    })
}

fn append_command_output(log_path: &Path, output: &std::process::Output) -> Result<()> {
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .with_context(|| format!("failed to open {}", log_path.display()))?;
    use std::io::Write;
    if !output.stdout.is_empty() {
        log.write_all(&output.stdout)
            .with_context(|| format!("failed to write {}", log_path.display()))?;
    }
    if !output.stderr.is_empty() {
        log.write_all(&output.stderr)
            .with_context(|| format!("failed to write {}", log_path.display()))?;
    }
    Ok(())
}

fn wait_for_public_url(
    log_path: &Path,
    provider: ShareProvider,
    timeout: Duration,
) -> Option<String> {
    let started = std::time::Instant::now();
    while started.elapsed() < timeout {
        if let Some(url) = public_url_from_log(log_path, provider) {
            return Some(url);
        }
        thread::sleep(Duration::from_millis(200));
    }
    None
}

fn public_url_from_log(log_path: &Path, provider: ShareProvider) -> Option<String> {
    let mut content = String::new();
    let mut file = std::fs::File::open(log_path).ok()?;
    file.read_to_string(&mut content).ok()?;
    extract_provider_url(&content, provider)
}

pub(super) fn extract_provider_url(content: &str, provider: ShareProvider) -> Option<String> {
    content
        .split_whitespace()
        .map(trim_url_token)
        .find(|token| token.starts_with("https://") && provider.public_url_matches(token))
        .map(ToOwned::to_owned)
}

fn trim_url_token(token: &str) -> &str {
    token.trim_matches(|ch: char| {
        ch == ',' || ch == ';' || ch == ')' || ch == ']' || ch == '"' || ch == '\''
    })
}

fn hydrate_public_urls(sessions: &mut [ShareSession]) {
    for session in sessions {
        if session.public_url.is_some() {
            continue;
        }

        let url =
            public_url_from_log(Path::new(&session.log_path), session.provider).or_else(|| {
                if session.provider == ShareProvider::Tailscale {
                    tailscale_public_url_from_status()
                } else {
                    None
                }
            });
        let Some(url) = url else {
            continue;
        };

        session.public_url = Some(url);
    }
}

fn matches_filter(
    session: &ShareSession,
    service: Option<&str>,
    provider: Option<ShareProvider>,
) -> bool {
    let provider_matches = provider.is_none_or(|expected| session.provider == expected);
    let service_matches = service.is_none_or(|expected| session.service == expected);
    provider_matches && service_matches
}

fn session_running_for(session: &ShareSession) -> bool {
    match session.provider {
        ShareProvider::Tailscale => tailscale_funnel_matches(&session.local_url),
        ShareProvider::Cloudflare | ShareProvider::Expose => {
            session.pid.is_some_and(pid_is_running)
        }
    }
}

fn pid_is_running(pid: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .is_ok_and(|status| status.success())
}

fn stop_pid(pid: u32) -> Result<()> {
    if !pid_is_running(pid) {
        return Ok(());
    }

    let status = Command::new("kill")
        .arg(pid.to_string())
        .status()
        .context("failed to stop share process")?;

    if !status.success() {
        anyhow::bail!("failed to stop share process pid {pid}");
    }

    Ok(())
}

fn tailscale_funnel_matches(local_url: &str) -> bool {
    let output = Command::new(provider_binary(ShareProvider::Tailscale))
        .arg("funnel")
        .arg("status")
        .output();
    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    text.contains(local_url) && text.contains("Funnel on")
}

fn tailscale_public_url_from_status() -> Option<String> {
    let output = Command::new(provider_binary(ShareProvider::Tailscale))
        .args(["funnel", "status"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    extract_provider_url(
        &String::from_utf8_lossy(&output.stdout),
        ShareProvider::Tailscale,
    )
}

fn stop_tailscale_funnel() -> Result<()> {
    let output = Command::new(provider_binary(ShareProvider::Tailscale))
        .args(["funnel", "--https=443", "off"])
        .output()
        .context("failed to stop tailscale funnel")?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("No serve config") {
        return Ok(());
    }

    anyhow::bail!(
        "failed to stop tailscale funnel: {}",
        stderr.trim().to_owned()
    );
}

fn verify_public_session(session: &mut ShareSession, timeout: Duration) -> Result<()> {
    if session.public_url.is_none() && session.provider == ShareProvider::Tailscale {
        session.public_url = tailscale_public_url_from_status();
    }
    let Some(public_url) = session.public_url.as_deref() else {
        anyhow::bail!(
            "share started but public URL was not discovered yet; check {}",
            session.log_path
        );
    };

    if !wait_for_public_response(public_url, timeout) {
        anyhow::bail!(
            "public tunnel URL did not respond within {}s: {}",
            timeout.as_secs(),
            public_url
        );
    }
    Ok(())
}

fn wait_for_public_response(url: &str, timeout: Duration) -> bool {
    let started = std::time::Instant::now();
    while started.elapsed() < timeout {
        if public_url_responds(url) {
            return true;
        }
        thread::sleep(Duration::from_millis(750));
    }
    false
}

fn public_url_responds(url: &str) -> bool {
    let output = Command::new("curl")
        .args([
            "-k",
            "-s",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "--connect-timeout",
            "3",
            "--max-time",
            "5",
            "-L",
            url,
        ])
        .output();
    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }

    let code = String::from_utf8_lossy(&output.stdout);
    code.trim().parse::<u16>().is_ok_and(|parsed| parsed > 0)
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use crate::config::{Driver, Kind, ServiceConfig};
    use crate::share::{ShareProvider, ShareSession};

    use super::{
        append_command_output, clear_test_provider_binaries, clear_test_state_home,
        command_for_state, extract_provider_url, matches_filter, provider_binary,
        public_url_from_log, read_state, remove_session, session_id, session_running_for,
        set_test_provider_binary, set_test_state_home, spawn_detached, start_session,
        state_paths_with_home, status_sessions, stop_pid, stop_sessions, trim_url_token,
        upsert_session, verify_public_session, wait_for_public_response, wait_for_public_url,
    };

    fn mock_binary(path: &Path, name: &str, body: &str) -> String {
        let bin_path = path.join(name);
        fs::write(&bin_path, body).expect("write mock binary");
        #[cfg(unix)]
        {
            let mut permissions = fs::metadata(&bin_path).expect("metadata").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&bin_path, permissions).expect("set permissions");
        }
        bin_path.to_string_lossy().to_string()
    }

    fn share_home_path(prefix: &str) -> PathBuf {
        let home = env::temp_dir().join(format!(
            "berth-share-state-{}-{}",
            prefix,
            std::process::id()
        ));
        drop(fs::remove_dir_all(&home));
        fs::create_dir_all(&home).expect("create home path");
        home
    }

    fn reset_share_test_context(home: &Path) -> PathBuf {
        clear_test_state_home();
        clear_test_provider_binaries();
        set_test_state_home(&home.to_str().expect("temporary home should be valid UTF-8"));
        home.to_path_buf()
    }

    fn test_service() -> ServiceConfig {
        ServiceConfig {
            name: "app".to_owned(),
            kind: Kind::App,
            driver: Driver::Frankenphp,
            image: "dunglas/frankenphp:php8.4".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 33065,
            database: None,
            username: None,
            password: None,
            bucket: None,
            access_key: None,
            secret_key: None,
            api_key: None,
            region: None,
            scheme: None,
            domain: Some("acme-api.berth".to_owned()),
            domains: None,
            resolved_domain: None,
            container_port: Some(80),
            smtp_port: None,
            volumes: None,
            env: None,
            command: None,
            depends_on: None,
            seed_file: None,
            hook: Vec::new(),
            health_path: None,
            health_statuses: None,
            restart: None,
            localhost_tls: false,
            octane: false,
            octane_workers: None,
            octane_max_requests: None,
            php_extensions: None,
            trust_container_ca: false,
            env_mapping: None,
            javascript: None,
            container_name: Some("acme-api-app".to_owned()),
            resolved_container_name: Some("acme-api-app".to_owned()),
            runtime: Default::default(),
        }
    }

    fn fake_session(
        id: &str,
        service: &str,
        provider: ShareProvider,
        pid: Option<u32>,
    ) -> ShareSession {
        ShareSession {
            id: id.to_owned(),
            provider,
            service: service.to_owned(),
            local_url: "http://127.0.0.1:33065".to_owned(),
            public_url: None,
            pid,
            log_path: "/tmp/berth-share.log".to_owned(),
            command: vec!["echo".to_owned()],
            started_at_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |duration| duration.as_secs()),
        }
    }

    #[test]
    fn state_paths_respects_home() {
        let home = env::temp_dir().join("berth-share-state-test-home");
        drop(fs::remove_dir_all(&home));
        fs::create_dir_all(&home).expect("create test home");
        let expected = home.join(".config/berth/share/sessions.toml");

        let home_str = home.to_string_lossy();
        let paths = state_paths_with_home(&home_str);
        assert_eq!(paths.state_path, expected);
        assert_eq!(paths.logs_dir, home.join(".config/berth/share/logs"));
    }

    #[test]
    fn command_and_session_id_are_deterministic() {
        assert_eq!(session_id(ShareProvider::Expose, "api"), "expose-api");
        assert_eq!(
            command_for_state(
                ShareProvider::Expose,
                &["share".to_owned(), "http://localhost".to_owned()]
            ),
            vec![
                "expose".to_owned(),
                "share".to_owned(),
                "http://localhost".to_owned()
            ]
        );
        assert_eq!(
            command_for_state(ShareProvider::Cloudflare, &[]),
            vec!["cloudflared".to_owned()]
        );
    }

    #[test]
    fn read_and_write_state_round_trip() {
        let temp_dir = env::temp_dir().join(format!(
            "berth-share-state-roundtrip-{}",
            std::process::id()
        ));
        drop(fs::remove_dir_all(&temp_dir));
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let state_path = temp_dir.join("sessions.toml");

        let first = ShareSession {
            id: "cloudflare-app".to_owned(),
            provider: ShareProvider::Cloudflare,
            service: "app".to_owned(),
            local_url: "http://127.0.0.1:33065".to_owned(),
            public_url: Some("https://example.trycloudflare.com".to_owned()),
            pid: Some(999),
            log_path: "/tmp/example.log".to_owned(),
            command: vec!["cloudflared".to_owned()],
            started_at_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |duration| duration.as_secs()),
        };

        upsert_session(&state_path, first.clone()).expect("upsert initial session");
        let state = read_state(&state_path).expect("read written state");
        assert_eq!(state.sessions.len(), 1);
        assert_eq!(state.sessions[0].id, first.id);

        let updated = ShareSession {
            started_at_unix: 0,
            ..first
        };
        upsert_session(&state_path, updated.clone()).expect("update existing session");
        let state = read_state(&state_path).expect("read updated state");
        assert_eq!(state.sessions.len(), 1);
        assert_eq!(state.sessions[0].id, updated.id);
        assert_eq!(state.sessions[0].started_at_unix, 0);
    }

    #[test]
    fn matches_filter_works_for_service_and_provider() {
        let session = fake_session("cloudflare-api", "api", ShareProvider::Cloudflare, Some(0));
        assert!(matches_filter(
            &session,
            Some("api"),
            Some(ShareProvider::Cloudflare)
        ));
        assert!(matches_filter(&session, Some("api"), None));
        assert!(!matches_filter(
            &session,
            Some("web"),
            Some(ShareProvider::Cloudflare)
        ));
        assert!(!matches_filter(
            &session,
            Some("api"),
            Some(ShareProvider::Expose)
        ));
    }

    #[test]
    fn parse_helpers_strip_trailing_punctuation() {
        assert_eq!(
            trim_url_token("https://example.dev/,"),
            "https://example.dev/"
        );
    }

    #[test]
    fn upsert_and_remove_session_update_state() {
        let temp_dir = env::temp_dir().join(format!(
            "berth-share-state-upsert-remove-{}",
            std::process::id()
        ));
        drop(fs::remove_dir_all(&temp_dir));
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let state_path = temp_dir.join("sessions.toml");

        let session_one = ShareSession {
            id: "cloudflare-app".to_owned(),
            provider: ShareProvider::Cloudflare,
            service: "app".to_owned(),
            local_url: "http://127.0.0.1:3000".to_owned(),
            public_url: Some("https://example.trycloudflare.com".to_owned()),
            pid: Some(123),
            log_path: temp_dir.join("app.log").to_string_lossy().to_string(),
            command: vec!["cloudflared".to_owned()],
            started_at_unix: 1,
        };
        let session_two = ShareSession {
            id: "expose-web".to_owned(),
            provider: ShareProvider::Expose,
            service: "web".to_owned(),
            local_url: "http://127.0.0.1:3001".to_owned(),
            public_url: Some("https://example.sharedwithexpose.com".to_owned()),
            pid: Some(124),
            log_path: temp_dir.join("web.log").to_string_lossy().to_string(),
            command: vec!["expose".to_owned()],
            started_at_unix: 2,
        };

        upsert_session(&state_path, session_one.clone()).expect("insert first");
        upsert_session(&state_path, session_two.clone()).expect("insert second");

        remove_session(&state_path, &session_one.id).expect("remove first");
        let state = read_state(&state_path).expect("read state");
        assert_eq!(state.sessions.len(), 1);
        assert_eq!(state.sessions[0].id, session_two.id);
    }

    #[test]
    fn reads_public_url_from_tailscale_like_log_output() {
        let temp_dir =
            env::temp_dir().join(format!("berth-share-state-log-{}", std::process::id()));
        drop(fs::remove_dir_all(&temp_dir));
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let log_path = temp_dir.join("tailscale.log");

        let mut file = fs::File::create(&log_path).expect("create log");
        writeln!(file, "some line").expect("write noise");
        writeln!(file, "tunnel available at https://example.com.ts.net)").expect("write url");

        let url = public_url_from_log(&log_path, ShareProvider::Tailscale).expect("tail url");
        assert_eq!(url, "https://example.com.ts.net");
    }

    #[test]
    fn extract_provider_url_matches_only_expected_provider() {
        assert!(
            extract_provider_url("https://example.trycloudflare.com", ShareProvider::Expose,)
                .is_none()
        );
        assert_eq!(
            extract_provider_url(
                "https://example.trycloudflare.com",
                ShareProvider::Cloudflare
            ),
            Some("https://example.trycloudflare.com".to_owned())
        );
    }

    #[test]
    fn wait_for_public_url_reads_when_log_file_populates() {
        let temp_dir =
            env::temp_dir().join(format!("berth-share-state-wait-{}", std::process::id()));
        drop(fs::remove_dir_all(&temp_dir));
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let log_path = temp_dir.join("wait.log");

        let writer_path = log_path.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(150));
            let mut file = fs::File::create(writer_path).expect("create log");
            file.write_all(b"https://example.trycloudflare.com\n")
                .expect("write");
        });

        let result = wait_for_public_url(
            &log_path,
            ShareProvider::Cloudflare,
            std::time::Duration::from_secs(2),
        );
        assert_eq!(result, Some("https://example.trycloudflare.com".to_owned()));
    }

    #[test]
    fn session_running_for_ignores_missing_process() {
        assert!(!session_running_for(&fake_session(
            "cloudflare-app",
            "app",
            ShareProvider::Cloudflare,
            Some(999_999),
        )));
        assert!(!session_running_for(&fake_session(
            "expose-app",
            "app",
            ShareProvider::Expose,
            None,
        )));
    }

    #[test]
    fn start_session_foreground_clears_state_after_exit() {
        let home = reset_share_test_context(&share_home_path("foreground-success"));
        let binary = mock_binary(&home, "cloudflare-success", "#!/usr/bin/env sh\nexit 0\n");
        set_test_provider_binary(ShareProvider::Cloudflare, &binary);

        let result = start_session(&test_service(), ShareProvider::Cloudflare, false, 1)
            .expect("foreground session start");
        assert_eq!(result.foreground_exit_code, Some(0));
        assert!(!result.session.running);

        let paths =
            state_paths_with_home(home.to_str().expect("temporary home should be valid UTF-8"));
        let state = read_state(&paths.state_path).expect("read state");
        assert_eq!(state.sessions.len(), 0);
    }

    #[test]
    fn start_session_reports_missing_provider_binary() {
        drop(reset_share_test_context(&share_home_path("missing-binary")));
        set_test_provider_binary(ShareProvider::Expose, "/does-not-exist");
        let error = match start_session(&test_service(), ShareProvider::Expose, false, 1) {
            Ok(_) => panic!("expected missing binary error"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("required provider binary '/does-not-exist'")
        );
    }

    #[test]
    fn status_sessions_hydrates_public_url_and_filters() {
        let home = reset_share_test_context(&share_home_path("status"));
        let paths =
            state_paths_with_home(home.to_str().expect("temporary home should be valid UTF-8"));

        let session = ShareSession {
            id: "cloudflare-app".to_owned(),
            provider: ShareProvider::Cloudflare,
            service: "api".to_owned(),
            local_url: "http://127.0.0.1:33065".to_owned(),
            public_url: None,
            pid: Some(999_999),
            log_path: paths.logs_dir.join("app.log").to_string_lossy().to_string(),
            command: vec!["cloudflared".to_owned()],
            started_at_unix: 0,
        };
        fs::create_dir_all(&paths.logs_dir).expect("create logs dir");
        fs::write(&session.log_path, "https://example.trycloudflare.com")
            .expect("write provider url");
        upsert_session(&paths.state_path, session.clone()).expect("seed state");

        let statuses =
            status_sessions(Some("api"), Some(ShareProvider::Cloudflare)).expect("status sessions");
        assert_eq!(statuses.len(), 1);
        assert_eq!(
            statuses[0].public_url,
            Some("https://example.trycloudflare.com".to_owned())
        );

        let all = status_sessions(Some("missing"), None).expect("status with missing service");
        assert!(all.is_empty());
    }

    #[test]
    fn stop_sessions_removes_filtered_sessions() {
        let home = reset_share_test_context(&share_home_path("stop"));
        let paths =
            state_paths_with_home(home.to_str().expect("temporary home should be valid UTF-8"));

        let keep = fake_session(
            "cloudflare-keep",
            "keep",
            ShareProvider::Cloudflare,
            Some(999_999),
        );
        let stop = fake_session(
            "cloudflare-stop",
            "stop",
            ShareProvider::Cloudflare,
            Some(999_999),
        );
        upsert_session(&paths.state_path, keep).expect("seed keep session");
        upsert_session(&paths.state_path, stop).expect("seed stop session");

        let removed = stop_sessions(Some("stop"), Some(ShareProvider::Cloudflare), false)
            .expect("stop sessions");
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].id, "cloudflare-stop");
        let state = read_state(&paths.state_path).expect("read state");
        assert_eq!(state.sessions.len(), 1);
        assert_eq!(state.sessions[0].id, "cloudflare-keep");
    }

    #[test]
    fn verify_public_session_requires_public_url() {
        let mut session = fake_session("cloudflare-app", "app", ShareProvider::Cloudflare, Some(1));
        session.public_url = None;
        let error = verify_public_session(&mut session, std::time::Duration::from_millis(10))
            .expect_err("expected missing public_url error");
        assert!(
            error
                .to_string()
                .contains("public URL was not discovered yet")
        );
    }

    #[test]
    fn wait_for_public_response_rejects_invalid_urls() {
        assert!(!wait_for_public_response(
            ":://invalid",
            std::time::Duration::from_millis(10)
        ));
    }

    #[test]
    fn provider_binary_prefers_custom_binary_over_default_and_updates_value() {
        let home = reset_share_test_context(&share_home_path("provider-binary"));
        assert_eq!(provider_binary(ShareProvider::Expose), "expose");

        let first = mock_binary(&home, "expose-first", "#!/usr/bin/env sh\nexit 0\n");
        set_test_provider_binary(ShareProvider::Expose, &first);
        assert_eq!(provider_binary(ShareProvider::Expose), first);

        let second = mock_binary(&home, "expose-second", "#!/usr/bin/env sh\nexit 0\n");
        set_test_provider_binary(ShareProvider::Expose, &second);
        assert_eq!(provider_binary(ShareProvider::Expose), second);
    }

    #[test]
    fn append_command_output_appends_stdout_and_stderr() {
        let temp_dir =
            env::temp_dir().join(format!("berth-share-append-output-{}", std::process::id()));
        drop(fs::remove_dir_all(&temp_dir));
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let log = temp_dir.join("command.log");

        let output = Command::new("sh")
            .args(["-c", "printf 'out'; printf 'err' >&2"])
            .output()
            .expect("run mock command");

        append_command_output(&log, &output).expect("append command output");
        let content = fs::read_to_string(&log).expect("read command log");
        assert_eq!(content, "outerr");
    }

    #[test]
    fn spawn_detached_starts_mock_binary() {
        let home = reset_share_test_context(&share_home_path("spawn-detached"));
        let binary = mock_binary(&home, "expose-detached", "#!/usr/bin/env sh\nsleep 60\n");
        set_test_provider_binary(ShareProvider::Expose, &binary);

        let temp_dir =
            env::temp_dir().join(format!("berth-share-state-spawn-{}", std::process::id()));
        drop(fs::remove_dir_all(&temp_dir));
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let log_path = temp_dir.join("detached.log");

        let pid = spawn_detached(ShareProvider::Expose, &[], &log_path).expect("spawn detached");
        assert!(pid > 0);
        assert!(super::pid_is_running(pid));
        stop_pid(pid).expect("stop detached process");
    }

    #[test]
    fn start_session_detached_missing_public_url_cleans_up_process() {
        let home = reset_share_test_context(&share_home_path("start-detached-failure"));
        let binary = mock_binary(
            &home,
            "cloudflare-detached-failure",
            "#!/usr/bin/env sh\nif [ \"$1\" = \"--version\" ]; then\n  exit 0\nfi\necho \"provider starting\"\n",
        );
        set_test_provider_binary(ShareProvider::Cloudflare, &binary);

        let error = match start_session(&test_service(), ShareProvider::Cloudflare, true, 1) {
            Ok(_) => panic!("expected detached start error"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("share started but public URL was not discovered yet")
        );
    }

    #[test]
    fn start_session_tailscale_backend_failure_includes_status() {
        let home = reset_share_test_context(&share_home_path("start-tailscale-failure"));
        let binary = mock_binary(
            &home,
            "tailscale-failure",
            "#!/usr/bin/env sh\nif [ \"$1\" = \"--version\" ]; then\n  exit 0\nfi\necho \"funnel failed\" >&2\nexit 1\n",
        );
        set_test_provider_binary(ShareProvider::Tailscale, &binary);

        let error = match start_session(&test_service(), ShareProvider::Tailscale, false, 1) {
            Ok(_) => panic!("expected tailscale error"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("tailscale funnel failed"));
        assert!(error.to_string().contains("funnel failed"));
    }

    #[test]
    fn stop_sessions_stops_tailscale_session_via_funnel_off() {
        let home = reset_share_test_context(&share_home_path("stop-tailscale"));
        let binary = mock_binary(
            &home,
            "tailscale-stop",
            "#!/usr/bin/env sh\nif [ \"$1\" = \"--version\" ]; then\n  exit 0\nelif [ \"$1\" = \"funnel\" ] && [ \"$2\" = \"--https=443\" ] && [ \"$3\" = \"off\" ]; then\n  exit 0\nfi\n",
        );
        set_test_provider_binary(ShareProvider::Tailscale, &binary);

        let paths =
            state_paths_with_home(home.to_str().expect("temporary home should be valid UTF-8"));
        let tail = fake_session("tailscale-app", "app", ShareProvider::Tailscale, None);
        let cloud = fake_session(
            "cloudflare-app",
            "web",
            ShareProvider::Cloudflare,
            Some(999_999),
        );
        upsert_session(&paths.state_path, tail).expect("seed tailscale session");
        upsert_session(&paths.state_path, cloud).expect("seed cloudflare session");

        let removed = stop_sessions(Some("app"), Some(ShareProvider::Tailscale), false)
            .expect("stop tailscale session");
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].id, "tailscale-app");
        let state = read_state(&paths.state_path).expect("read state after stop");
        assert_eq!(state.sessions.len(), 1);
        assert_eq!(state.sessions[0].id, "cloudflare-app");
    }

    #[test]
    fn session_running_for_tailscale_checks_funnel_status() {
        let home = reset_share_test_context(&share_home_path("tailscale-run"));
        let binary = mock_binary(
            &home,
            "tailscale",
            "#!/usr/bin/env sh\necho 'http://127.0.0.1:33065 Funnel on'\n",
        );
        set_test_provider_binary(ShareProvider::Tailscale, &binary);

        assert!(session_running_for(&fake_session(
            "tailscale-app",
            "app",
            ShareProvider::Tailscale,
            None
        )));
    }

    #[test]
    fn pid_is_running_and_stop_pid_kill_live_process() {
        let mut child = Command::new("/bin/sleep")
            .arg("60")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn sleep");

        let pid = child.id();
        assert!(super::pid_is_running(pid));
        stop_pid(pid).expect("stop pid");
        child.wait().expect("wait sleep");
        assert!(!super::pid_is_running(pid));
    }
}
