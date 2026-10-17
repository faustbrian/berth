//! Framework-boot readiness for Laravel apps.
//!
//! An HTTP `/up` answer only proves the web server runs: in 8.x it passed
//! while `bootstrap/app.php` was fatal (FI-06). `artisan about` boots the
//! whole framework (config, providers, environment), so it fails on exactly
//! what an agent's next command would hit.

use anyhow::{Context, Result};
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

/// One probe may take this long; a cold container's first boot compiles
/// and caches, which can take several seconds (8.x FI-60 used 15s).
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);
/// Pause between probes.
const PROBE_INTERVAL: Duration = Duration::from_secs(2);

/// Waits until `php artisan about --only=environment` succeeds in the app
/// container, failing at once when the container stops and otherwise after
/// `timeout_secs`, with the last probe output. Never restarts the container:
/// readiness is not liveness (8.x FI-60).
///
/// # Errors
///
/// Returns an error naming the app, the probe and its last output.
pub(crate) fn wait_until_laravel_boots(service: &ServiceConfig, timeout_secs: u64) -> Result<()> {
    let container = service.container_name()?;
    if crate::docker::is_dry_run() {
        output::event(
            &service.name,
            LogLevel::Info,
            "[dry-run] Wait for `php artisan about --only=environment` to succeed",
            Persistence::Transient,
        );
        return Ok(());
    }
    output::event(
        &service.name,
        LogLevel::Info,
        "Waiting for the framework to boot (php artisan about)",
        Persistence::Persistent,
    );
    let started = Instant::now();
    let deadline = started + Duration::from_secs(timeout_secs);
    loop {
        match crate::docker::inspect_status(&container).as_deref() {
            Some("running") => {}
            Some(status) => anyhow::bail!(
                "app service '{}' stopped while booting (container {container} is {status}); \
                 see `berth logs {}`",
                service.name,
                service.name
            ),
            None => anyhow::bail!(
                "app service '{}' has no container {container}",
                service.name
            ),
        }
        let last = match probe(&container)? {
            Probe::Booted => {
                output::event(
                    &service.name,
                    LogLevel::Success,
                    &format!("Framework booted ({}s)", started.elapsed().as_secs()),
                    Persistence::Persistent,
                );
                return Ok(());
            }
            Probe::Failed(output) => output,
            Probe::TimedOut => format!("no answer within {}s", PROBE_TIMEOUT.as_secs()),
        };
        if Instant::now() + PROBE_INTERVAL >= deadline {
            anyhow::bail!(
                "app service '{}' did not boot within {timeout_secs}s: \
                 `php artisan about --only=environment` failed: {}",
                service.name,
                tail(&last)
            );
        }
        std::thread::sleep(PROBE_INTERVAL);
    }
}

enum Probe {
    Booted,
    Failed(String),
    TimedOut,
}

fn probe(container: &str) -> Result<Probe> {
    match run_artisan(container, &["about", "--only=environment", "--no-ansi"])? {
        // `about` exists since Laravel 9.21; older apps still boot the
        // console kernel (config, providers) for `--version`.
        Probe::Failed(output)
            if output.contains("Command \"about\" is not defined")
                || output.contains("\"--only\" option does not exist") =>
        {
            run_artisan(container, &["--version", "--no-ansi"])
        }
        other => Ok(other),
    }
}

fn run_artisan(container: &str, args: &[&str]) -> Result<Probe> {
    let mut child = Command::new(crate::docker::docker_command())
        .args(["exec", container, "php", "artisan"])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to run the framework boot probe")?;
    // Drain both pipes while the probe runs: a long stack trace or
    // deprecation flood would otherwise fill a pipe and block the child.
    let stdout = child.stdout.take().map(drain);
    let stderr = child.stderr.take().map(drain);
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .context("failed to wait for the boot probe")?
        {
            break Some(status);
        }
        if started.elapsed() >= PROBE_TIMEOUT {
            drop(child.kill());
            drop(child.wait());
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let mut text = stdout.map(collect).unwrap_or_default();
    text.push_str(&stderr.map(collect).unwrap_or_default());
    Ok(match status {
        Some(status) if status.success() => Probe::Booted,
        Some(_) => Probe::Failed(text),
        None => Probe::TimedOut,
    })
}

fn drain<R: Read + Send + 'static>(mut pipe: R) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        drop(pipe.read_to_end(&mut bytes));
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

fn collect(handle: std::thread::JoinHandle<String>) -> String {
    handle.join().unwrap_or_default()
}

/// The last few non-empty lines of probe output, joined for one message.
fn tail(text: &str) -> String {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let start = lines.len().saturating_sub(5);
    let joined = lines[start..].join(" | ");
    if joined.is_empty() {
        "no output".to_owned()
    } else {
        joined
    }
}

#[cfg(test)]
mod tests {
    use super::tail;

    #[test]
    fn the_failure_message_keeps_the_last_lines() {
        let output = "a\n\nb\nc\nd\ne\nPHP Fatal error: boom in bootstrap/app.php\n";
        assert_eq!(
            tail(output),
            "b | c | d | e | PHP Fatal error: boom in bootstrap/app.php"
        );
        assert_eq!(tail("  \n"), "no output");
    }
}
