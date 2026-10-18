//! Shared docker image pull command helper.

use anyhow::{Context, Result};
use std::io::Write;
use std::process::Stdio;
use std::time::Duration;

/// Attempts per pull. 30 agents pulling one image at once can hit registry
/// rate limits and dropped connections; a pull either succeeds within a few
/// tries or reports the registry's own error (ledger L027, FI-34).
const PULL_ATTEMPTS: u32 = 3;

/// Registry and network failures worth another attempt. Anything else (an
/// unknown image, a denied repository, a bad reference) fails at once.
const TRANSIENT_PULL_ERRORS: [&str; 12] = [
    "toomanyrequests",
    "TLS handshake timeout",
    "i/o timeout",
    "connection reset by peer",
    "connection refused",
    "context deadline exceeded",
    "net/http: request canceled",
    "unexpected EOF",
    "502 Bad Gateway",
    "503 Service Unavailable",
    "504 Gateway Timeout",
    "500 Internal Server Error",
];

/// Executes `docker pull <image>`, retrying transient registry and network
/// failures with a short backoff, and fails with the last attempt's stderr.
///
/// # Errors
///
/// Returns an error when Docker cannot be started or every attempt fails.
pub(crate) fn docker_pull(image: &str, context: &str, failure_message: &str) -> Result<()> {
    let mut attempt = 1;
    loop {
        let stderr = pull_once(image, context)?;
        let Some(stderr) = stderr else {
            return Ok(());
        };
        if attempt >= PULL_ATTEMPTS || !is_transient(&stderr) {
            anyhow::bail!("{failure_message}: {}", stderr.trim());
        }
        crate::output::event(
            "pull",
            crate::output::LogLevel::Warn,
            &format!(
                "Pulling {image} failed ({}); retrying ({}/{PULL_ATTEMPTS})",
                stderr.trim(),
                attempt + 1
            ),
            crate::output::Persistence::Persistent,
        );
        std::thread::sleep(backoff(attempt));
        attempt += 1;
    }
}

/// One pull with progress on berth's child stdout and stderr captured,
/// returning the stderr of a failed pull.
fn pull_once(image: &str, context: &str) -> Result<Option<String>> {
    let output = super::secret_env::docker_process(&["pull", image])?
        .stdout(crate::output::child_stdout())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| context.to_owned())?;
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if output.status.success() {
        // Warnings of a pull that worked (a platform mismatch) still show.
        drop(std::io::stderr().write_all(stderr.as_bytes()));
        return Ok(None);
    }
    // A failure's stderr is shown once, in the retry warning or the error.
    Ok(Some(stderr))
}

fn is_transient(stderr: &str) -> bool {
    TRANSIENT_PULL_ERRORS
        .iter()
        .any(|marker| stderr.contains(marker))
}

fn backoff(attempt: u32) -> Duration {
    if cfg!(test) {
        return Duration::from_millis(1);
    }
    Duration::from_secs(2_u64.pow(attempt))
}

#[cfg(test)]
mod tests {
    use crate::docker;
    use std::fs;
    use std::io::Write;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    fn with_fake_docker<F, T>(script: &str, test: F) -> T
    where
        F: FnOnce() -> T,
    {
        let bin_dir = crate::test_support::temp_root().join(format!(
            "berth-image-pull-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        // A clock tick is coarse: two parallel tests could share a name and
        // remove each other's fake binary.
        let bin_dir = docker::unique_test_path(bin_dir);
        fs::create_dir_all(&bin_dir).expect("create fake docker dir");
        let binary = bin_dir.join("docker");
        let mut file = fs::File::create(&binary).expect("write fake docker");
        writeln!(file, "#!/bin/sh\n{}", script).expect("script");
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&binary).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&binary, perms).expect("chmod");
        }

        let binary = binary.to_string_lossy().to_string();
        let result =
            docker::with_dry_run_state(false, || docker::with_docker_command(&binary, test));
        fs::remove_dir_all(&bin_dir).ok();
        result
    }

    #[test]
    fn docker_pull_succeeds_when_docker_succeeds() {
        with_fake_docker("exit 0", || {
            super::docker_pull("redis:7", "pull image", "pull failed").expect("image pull");
        });
    }

    #[test]
    fn transient_failures_are_retried_then_succeed() {
        let counter = crate::test_support::temp_root()
            .join(format!("berth-pull-count-{}", std::process::id()));
        fs::remove_file(&counter).ok();
        let script = format!(
            "n=$(cat {c} 2>/dev/null || echo 0); n=$((n+1)); echo $n > {c}\n\
             [ $n -ge 3 ] && exit 0\n\
             echo 'Error response from daemon: toomanyrequests: rate limited' >&2; exit 1",
            c = counter.display()
        );
        with_fake_docker(&script, || {
            super::docker_pull("redis:7", "pull image", "pull failed").expect("third attempt");
        });
        assert_eq!(fs::read_to_string(&counter).expect("count").trim(), "3");
        fs::remove_file(&counter).ok();
    }

    #[test]
    fn transient_failures_stop_after_three_attempts_with_the_last_error() {
        let counter = crate::test_support::temp_root()
            .join(format!("berth-pull-give-up-{}", std::process::id()));
        fs::remove_file(&counter).ok();
        let script = format!(
            "n=$(cat {c} 2>/dev/null || echo 0); n=$((n+1)); echo $n > {c}\n\
             echo \"net/http: TLS handshake timeout (attempt $n)\" >&2; exit 1",
            c = counter.display()
        );
        let error = with_fake_docker(&script, || {
            super::docker_pull("redis:7", "pull image", "pull failed").expect_err("gives up")
        });
        assert_eq!(fs::read_to_string(&counter).expect("count").trim(), "3");
        assert!(error.to_string().contains("(attempt 3)"), "{error}");
        fs::remove_file(&counter).ok();
    }

    #[test]
    fn permanent_failures_are_not_retried() {
        let counter = crate::test_support::temp_root()
            .join(format!("berth-pull-permanent-{}", std::process::id()));
        fs::remove_file(&counter).ok();
        let script = format!(
            "n=$(cat {c} 2>/dev/null || echo 0); n=$((n+1)); echo $n > {c}\n\
             echo 'Error response from daemon: manifest for redis:nope not found' >&2; exit 1",
            c = counter.display()
        );
        let error = with_fake_docker(&script, || {
            super::docker_pull("redis:nope", "pull image", "pull failed").expect_err("fails")
        });
        assert_eq!(fs::read_to_string(&counter).expect("count").trim(), "1");
        assert!(error.to_string().contains("not found"), "{error}");
        fs::remove_file(&counter).ok();
    }

    #[test]
    fn docker_pull_returns_error_when_docker_fails() {
        with_fake_docker("exit 1", || {
            let result = super::docker_pull("redis:7", "pull image", "pull failed");
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("pull failed"));
        });
    }
}
