//! Framework-boot readiness for Laravel apps.
//!
//! An HTTP `/up` answer only proves the web server runs: in 8.x it passed
//! while `bootstrap/app.php` was fatal (FI-06). `artisan about` boots the
//! whole framework (config, providers, environment), so it fails on exactly
//! what an agent's next command would hit.

use anyhow::{Context, Result};
use std::io::Read;
use std::process::Stdio;
use std::time::{Duration, Instant};

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

/// Pause between probes.
const PROBE_INTERVAL: Duration = Duration::from_secs(2);

/// Waits until `php artisan about --only=environment` succeeds in the app
/// container. It fails at once when the container stops or the probe hits a
/// PHP fatal error (8.x FI-06: a broken `bootstrap/app.php` never boots), and
/// otherwise only after an opt-in `timeout_secs`, with the last probe
/// output: a cold first boot on a loaded machine is slow, not broken. Never
/// restarts the container: readiness is not liveness (8.x FI-60).
///
/// # Errors
///
/// Returns an error naming the app, the probe and its last output.
pub(crate) fn wait_until_laravel_boots(
    service: &ServiceConfig,
    timeout_secs: Option<u64>,
) -> Result<()> {
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
    let deadline = timeout_secs.map(|secs| started + Duration::from_secs(secs));
    let cap = deadline;
    let mut last = String::new();
    loop {
        crate::parallel::ensure_no_sibling_failed()?;
        if cap.is_some_and(|cap| Instant::now() >= cap) {
            anyhow::bail!(
                "app service '{}' did not boot within the --wait-timeout{}{}",
                service.name,
                timeout_secs
                    .map(|secs| format!(" of {secs}s"))
                    .unwrap_or_default(),
                last_probe(&last)
            );
        }
        match crate::docker::container_state(&container) {
            crate::docker::ContainerState::Running | crate::docker::ContainerState::Unknown => {}
            crate::docker::ContainerState::NotRunning(status) => anyhow::bail!(
                "app service '{}' stopped while booting (container {container} is {status}); \
                 see why with {}, fix it and run {} again",
                service.name,
                crate::followup::logs(service),
                crate::followup::for_service(service, "up")
            ),
            crate::docker::ContainerState::Missing => anyhow::bail!(
                "app service '{}' has no container {container}: it was removed while berth \
                 waited for it (another `berth down`?); run {} again",
                service.name,
                crate::followup::for_service(service, "up")
            ),
        }
        match probe(&container, cap)? {
            Probe::Booted => {
                output::event(
                    &service.name,
                    LogLevel::Success,
                    &format!("Framework booted ({}s)", started.elapsed().as_secs()),
                    Persistence::Persistent,
                );
                return Ok(());
            }
            // `artisan` ran and failed: the app's code, config or `vendor/`
            // is broken (a PHP fatal, or an exception Laravel's console
            // kernel caught and rendered), and probing again gives the same
            // answer.
            // A container that stopped during the probe is reported as such,
            // and while the engine cannot even say whether it runs, the
            // failure may be the engine's: ask again.
            Probe::Failed(_)
                if !matches!(
                    crate::docker::container_state(&container),
                    crate::docker::ContainerState::Running
                ) => {}
            Probe::Failed(output) => anyhow::bail!(
                "app service '{}' cannot boot: `php artisan about --only=environment` failed: {}; \
                 fix that error (a missing vendor/ needs {}) and run {} again",
                service.name,
                tail(&output),
                crate::followup::for_service(
                    service,
                    &format!("composer --service {} install", service.name)
                ),
                crate::followup::for_service(service, "up")
            ),
            // The engine could not run the probe (the container is being
            // restarted, an overloaded engine): its state decides next.
            Probe::Unreached(output) => last = output,
            Probe::CapReached => anyhow::bail!(
                "app service '{}' did not boot within the --wait-timeout{}{}",
                service.name,
                timeout_secs
                    .map(|secs| format!(" of {secs}s"))
                    .unwrap_or_default(),
                last_probe(&last)
            ),
        }
        std::thread::sleep(PROBE_INTERVAL);
    }
}

/// The last probe's output for a capped wait's error, if there was any.
fn last_probe(output: &str) -> String {
    if output.trim().is_empty() {
        String::new()
    } else {
        format!("; the last probe said: {}", tail(output))
    }
}

enum Probe {
    Booted,
    Failed(String),
    Unreached(String),
    CapReached,
}

fn probe(container: &str, cap: Option<Instant>) -> Result<Probe> {
    match run_artisan(
        container,
        &["about", "--only=environment", "--no-ansi"],
        cap,
    )? {
        // `about` exists since Laravel 9.21; older apps still boot the
        // console kernel (config, providers) for `--version`.
        Probe::Failed(output)
            if output.contains("Command \"about\" is not defined")
                || output.contains("\"--only\" option does not exist") =>
        {
            run_artisan(container, &["--version", "--no-ansi"], cap)
        }
        other => Ok(other),
    }
}

fn run_artisan(container: &str, args: &[&str], cap: Option<Instant>) -> Result<Probe> {
    let mut child = crate::docker::engine_command()?
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
    // No deadline on one probe either: a cold first boot on a loaded
    // machine takes as long as it takes. Only an opt-in cap ends it.
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .context("failed to wait for the boot probe")?
        {
            break status;
        }
        if cap.is_some_and(|cap| Instant::now() >= cap) {
            drop(child.kill());
            drop(child.wait());
            return Ok(Probe::CapReached);
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let mut text = stdout.map(collect).unwrap_or_default();
    text.push_str(&stderr.map(collect).unwrap_or_default());
    Ok(if status.success() {
        Probe::Booted
    } else if crate::docker::is_engine_exec_failure(&text) {
        Probe::Unreached(text)
    } else {
        Probe::Failed(text)
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
