//! docker health module.
//!
//! Contains docker health logic used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::{inspect_status, is_dry_run};
use checks::check_service_health;

mod checks;
mod http;

/// Waits for until healthy to reach a ready state.
pub fn wait_until_healthy(
    service: &ServiceConfig,
    timeout: u64,
    interval: u64,
    retries: Option<u32>,
) -> Result<()> {
    let container_name = service.container_name()?;

    if is_dry_run() {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!(
                "[dry-run] Wait for service health (timeout={}s interval={}s retries={retries:?})",
                timeout, interval
            ),
            Persistence::Transient,
        );
        return Ok(());
    }

    let start = std::time::Instant::now();
    let timeout_dur = std::time::Duration::from_secs(timeout);
    let sleep_secs = interval.max(1);

    match inspect_status(&container_name) {
        Some(status) if status == "running" => {}
        Some(status) => {
            anyhow::bail!("Container '{container_name}' is not running (status: {status})")
        }
        None => anyhow::bail!("Container '{container_name}' does not exist"),
    }

    output::event(
        &service.name,
        LogLevel::Info,
        "Waiting for service to accept connections",
        Persistence::Persistent,
    );

    let mut attempts: u32 = 0;
    loop {
        attempts = attempts.saturating_add(1);

        match check_service_health(service, &container_name) {
            Ok(true) => {
                output::event(
                    &service.name,
                    LogLevel::Success,
                    "Service is ready",
                    Persistence::Persistent,
                );
                return Ok(());
            }
            Err(err) => {
                if retries.is_some_and(|max| attempts >= max) {
                    anyhow::bail!(
                        "Failed health check for '{}' after {} retries: {}",
                        service.name,
                        attempts,
                        err
                    );
                }
            }
            Ok(false) => {}
        }

        if retries.is_some_and(|max| attempts >= max) {
            anyhow::bail!(
                "Failed health check for '{}' after {} retries",
                service.name,
                attempts
            );
        }

        if start.elapsed() >= timeout_dur {
            anyhow::bail!(
                "Timed out after {}s waiting for '{}' to become ready",
                timeout,
                service.name
            );
        }

        std::thread::sleep(std::time::Duration::from_secs(sleep_secs));
    }
}
