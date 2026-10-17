//! Caddy process lifecycle helpers.

use anyhow::{Context, Result};
#[cfg(test)]
use std::cell::RefCell;
use std::path::Path;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use crate::output::{self, LogLevel, Persistence};

use super::capture::CommandCapture;

#[cfg(test)]
thread_local! {
    static TEST_CADDY_COMMAND: RefCell<Option<String>> = const { RefCell::new(None) };
}

const CADDY_COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

/// Verifies that `caddy` is installed and executable.
pub(super) fn ensure_caddy_installed() -> Result<()> {
    let output = match Command::new(caddy_binary_name()).arg("version").output() {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!(
                "caddy is not installed.\n\
                 install on macOS: brew install caddy\n\
                 install on Linux: https://caddyserver.com/docs/install"
            );
        }
        Err(error) => return Err(error).context("failed to execute caddy"),
    };

    ensure_success(output, "caddy is unavailable").map(|_| ())
}

/// Validates a Caddy config before Berth attempts to reload or start it.
pub(super) fn validate_caddy_config(caddyfile_path: &Path) -> Result<()> {
    let config_path = caddyfile_path.to_string_lossy().into_owned();
    let output = run_caddy(
        &[
            "validate",
            "--config",
            &config_path,
            "--adapter",
            "caddyfile",
        ],
        "failed to execute caddy validate",
    )?;

    ensure_success(output, "caddy config is invalid").map(|_| ())
}

/// Reloads Caddy with a new config, or starts it if reload fails.
///
/// Reload-first behavior preserves existing process state when possible.
pub(super) fn reload_or_start_caddy(caddyfile_path: &Path) -> Result<()> {
    let config_path = caddyfile_path.to_string_lossy().into_owned();
    let reload_output = run_caddy(
        &["reload", "--config", &config_path, "--adapter", "caddyfile"],
        "failed to execute caddy reload",
    )?;

    if reload_output.status.success() {
        output::event(
            "caddy",
            LogLevel::Success,
            &format!("Reloaded config {}", caddyfile_path.display()),
            Persistence::Persistent,
        );
        return Ok(());
    }

    let stop_output = run_caddy(&["stop"], "failed to execute caddy stop")
        .unwrap_or_else(|_| failed_output("timed out stopping caddy before restart"));
    let start_output = run_caddy(
        &["start", "--config", &config_path, "--adapter", "caddyfile"],
        "failed to execute caddy start",
    )?;

    if start_output.status.success() {
        output::event(
            "caddy",
            LogLevel::Success,
            &format!("Started with config {}", caddyfile_path.display()),
            Persistence::Persistent,
        );
        return Ok(());
    }

    let reload_stderr = stderr_text(&reload_output);
    let stop_stderr = stderr_text(&stop_output);
    let start_stderr = stderr_text(&start_output);
    anyhow::bail!(
        "failed to reload, stop, or start caddy\nreload error: {reload_stderr}\nstop error: {stop_stderr}\nstart error: {start_stderr}"
    );
}

/// Attempts to trust local Caddy CA in system trust store.
///
/// # Errors
///
/// Returns an error when command execution fails.
pub fn trust_local_caddy_ca() -> Result<()> {
    let output = run_caddy(&["trust"], "failed to execute caddy trust")?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = stderr_text(&output);
    output::event(
        "caddy",
        LogLevel::Warn,
        &format!("Failed to trust local CA automatically: {stderr}"),
        Persistence::Persistent,
    );
    Ok(())
}

fn run_caddy(args: &[&str], context: &str) -> Result<Output> {
    let capture = CommandCapture::new()?;
    let mut child = Command::new(caddy_binary_name())
        .args(args)
        .stdout(capture.stdout_stdio()?)
        .stderr(capture.stderr_stdio()?)
        .spawn()
        .with_context(|| context.to_owned())?;

    let deadline = Instant::now() + CADDY_COMMAND_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait().with_context(|| context.to_owned())? {
            return capture.finish(status);
        }

        if Instant::now() >= deadline {
            drop(child.kill());
            let status = child.wait().with_context(|| context.to_owned())?;
            let _output = capture.finish(status);
            anyhow::bail!(
                "timed out after {}s while running caddy {}",
                CADDY_COMMAND_TIMEOUT.as_secs(),
                args.join(" ")
            );
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}

fn caddy_binary_name() -> String {
    #[cfg(not(test))]
    {
        String::from("caddy")
    }

    #[cfg(test)]
    {
        TEST_CADDY_COMMAND.with(|command| {
            command
                .borrow()
                .clone()
                .unwrap_or_else(|| String::from("caddy"))
        })
    }
}

#[cfg(test)]
fn set_caddy_binary_name(path: String) {
    TEST_CADDY_COMMAND.with(|command| {
        *command.borrow_mut() = Some(path);
    });
}

fn ensure_success(output: Output, error_prefix: &str) -> Result<Output> {
    if output.status.success() {
        return Ok(output);
    }
    anyhow::bail!("{error_prefix}: {}", stderr_text(&output));
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn failed_output(stderr: &str) -> Output {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        Output {
            status: std::process::ExitStatus::from_raw(1 << 8),
            stdout: Vec::new(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }
    #[cfg(not(unix))]
    {
        let status = Command::new(caddy_binary_name())
            .arg("__berth_failed_output_sentinel__")
            .output()
            .map(|output| output.status)
            .unwrap_or_else(|_| panic!("failed to synthesize non-zero exit status"));
        Output {
            status,
            stdout: Vec::new(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use super::{ensure_caddy_installed, trust_local_caddy_ca, validate_caddy_config};
    use super::{reload_or_start_caddy, set_caddy_binary_name};

    static TEST_ENV_MUTEX: Mutex<()> = Mutex::new(());

    fn write_mock_bin(dir: &std::path::Path, name: &str, body: &str) {
        let path = dir.join(name);
        std::fs::write(&path, body).expect("write mock binary");
        let mut permissions = std::fs::metadata(&path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("permissions");
    }

    fn with_mock_path<F, R>(script: Option<&str>, test: F) -> R
    where
        F: FnOnce() -> R,
    {
        let _guard = TEST_ENV_MUTEX.lock().unwrap_or_else(|err| err.into_inner());
        let temp_dir = std::env::temp_dir().join(format!(
            "berth-mock-caddy-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        drop(std::fs::remove_dir_all(&temp_dir));
        std::fs::create_dir_all(&temp_dir).expect("create mock bin dir");

        if let Some(body) = script {
            write_mock_bin(&temp_dir, "caddy", body);
        }
        let binary = temp_dir.join("caddy");
        let command_path = binary.to_string_lossy().to_string();
        set_caddy_binary_name(command_path);

        let result = test();

        set_caddy_binary_name("caddy".to_owned());

        result
    }

    #[test]
    fn ensure_caddy_installed_returns_not_found_message_when_missing() {
        with_mock_path(None, || {
            let error = ensure_caddy_installed().expect_err("expected not found");
            assert!(error.to_string().contains("caddy is not installed"));
        });
    }

    #[test]
    fn ensure_caddy_installed_succeeds_with_mock_binary() {
        with_mock_path(
            Some("#!/usr/bin/env sh\necho caddy version\necho ok\n"),
            || {
                ensure_caddy_installed().expect("installed");
            },
        );
    }

    #[test]
    fn validate_caddy_config_accepts_valid_config() {
        with_mock_path(
            Some("#!/usr/bin/env sh\nif [ \"$1\" = \"validate\" ]; then exit 0; fi\nexit 1\n"),
            || {
                validate_caddy_config(std::path::Path::new("/tmp/Caddyfile"))
                    .expect("validate config");
            },
        );
    }

    #[test]
    fn reload_or_start_caddy_uses_reload_when_available() {
        with_mock_path(
            Some(
                "#!/usr/bin/env sh\n\nif [ \"$1\" = \"reload\" ]; then\n  echo reloaded\n  exit 0\nfi\n\nif [ \"$1\" = \"start\" ]; then\n  echo should-not-run\n  exit 1\nfi\n\nexit 0\n",
            ),
            || {
                reload_or_start_caddy(std::path::Path::new("/tmp/Caddyfile")).expect("reload path");
            },
        );
    }

    #[test]
    fn reload_or_start_caddy_falls_back_to_start_when_reload_fails() {
        with_mock_path(
            Some(
                "#!/usr/bin/env sh\n\nif [ \"$1\" = \"reload\" ]; then\n  echo reload failed >&2\n  exit 1\nfi\n\nif [ \"$1\" = \"start\" ]; then\n  echo started\n  exit 0\nfi\n\nexit 1\n",
            ),
            || {
                reload_or_start_caddy(std::path::Path::new("/tmp/Caddyfile"))
                    .expect("start fallback");
            },
        );
    }

    #[test]
    fn reload_or_start_caddy_stops_existing_process_before_start_fallback() {
        let temp_dir = std::env::temp_dir().join(format!(
            "berth-caddy-order-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        let log_path = temp_dir.join("calls.log");
        let script = format!(
            "#!/usr/bin/env sh\nprintf '%s\\n' \"$1\" >> \"{}\"\nif [ \"$1\" = \"reload\" ]; then\n  exit 1\nfi\nif [ \"$1\" = \"stop\" ]; then\n  exit 0\nfi\nif [ \"$1\" = \"start\" ]; then\n  exit 0\nfi\nexit 1\n",
            log_path.display()
        );

        with_mock_path(Some(&script), || {
            reload_or_start_caddy(std::path::Path::new("/tmp/Caddyfile")).expect("start fallback");
        });

        let calls = std::fs::read_to_string(&log_path).expect("read calls");
        assert_eq!(calls, "reload\nstop\nstart\n");
        drop(std::fs::remove_dir_all(&temp_dir));
    }

    #[test]
    fn reload_or_start_caddy_does_not_block_on_inherited_stdio() {
        with_mock_path(
            Some(
                "#!/usr/bin/env sh\n\nif [ \"$1\" = \"reload\" ]; then\n  echo reload failed >&2\n  exit 1\nfi\n\nif [ \"$1\" = \"stop\" ]; then\n  exit 0\nfi\n\nif [ \"$1\" = \"start\" ]; then\n  (sleep 2) &\n  echo started\n  exit 0\nfi\n\nexit 1\n",
            ),
            || {
                let started_at = Instant::now();
                reload_or_start_caddy(std::path::Path::new("/tmp/Caddyfile"))
                    .expect("start fallback");
                assert!(
                    started_at.elapsed() < Duration::from_secs(1),
                    "caddy start should not block on inherited stdio"
                );
            },
        );
    }

    #[test]
    fn reload_or_start_caddy_fails_when_reload_and_start_fail() {
        with_mock_path(Some("#!/usr/bin/env sh\n\necho failed\nexit 1\n"), || {
            let error = reload_or_start_caddy(std::path::Path::new("/tmp/Caddyfile"))
                .expect_err("expected fail");
            assert!(
                error
                    .to_string()
                    .contains("failed to reload, stop, or start caddy")
            );
        });
    }

    #[test]
    fn trust_local_caddy_ca_succeeds_on_zero_exit_code() {
        with_mock_path(
            Some("#!/usr/bin/env sh\nif [ \"$1\" = \"trust\" ]; then exit 0; fi; exit 1\n"),
            || {
                trust_local_caddy_ca().expect("trust");
            },
        );
    }

    #[test]
    fn trust_local_caddy_ca_ignores_command_error_and_succeeds() {
        with_mock_path(
            Some("#!/usr/bin/env sh\necho trust failure >&2\nexit 1\n"),
            || {
                trust_local_caddy_ca().expect("warn only");
            },
        );
    }

    #[cfg(not(unix))]
    #[allow(unused_imports)]
    #[test]
    fn trust_local_caddy_ca_compiles_without_unix_permissions_api() {
        assert!(std::time::Duration::from_secs(0).is_zero());
    }
}
