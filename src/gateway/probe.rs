//! Health probes for services without host ports, run inside the gateway
//! container after attaching it to the service's instance network.

use anyhow::Result;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use crate::config::ServiceConfig;

use super::GatewayPorts;
use super::container::{GatewayDirs, gateway_container_name};

/// Networks this process already attached the gateway to for probing.
static ATTACHED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

/// Set by inspection commands (`health`): probes then never start or attach
/// the gateway, so inspecting changes no Docker state.
static INSPECT_ONLY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Makes probes in this process use the gateway only if it already runs and
/// is attached.
pub(crate) fn probe_without_starting_gateway() {
    INSPECT_ONLY.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// The gateway could not be prepared for probing. Health waits stop on it
/// at once instead of retrying until their timeout, so the cause surfaces.
#[derive(Debug)]
pub(crate) struct ProbeSetupError(pub(crate) String);

impl std::fmt::Display for ProbeSetupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ProbeSetupError {}

/// Returns whether the service's container port accepts TCP connections.
///
/// # Errors
///
/// Returns an error when the gateway cannot be started or attached, or
/// Docker fails.
pub(crate) fn tcp_open(service: &ServiceConfig) -> Result<bool> {
    let (gateway, target) = prepare(service)?;
    let output = crate::docker::run_docker_output_owned(
        &[
            "exec".to_owned(),
            gateway,
            "nc".to_owned(),
            "-z".to_owned(),
            "-w".to_owned(),
            "2".to_owned(),
            target,
            service.resolved_container_port().to_string(),
        ],
        "failed to run a gateway health probe",
    )?;
    Ok(output.status.success())
}

/// Returns the HTTP status the service answers `path` with, or `None` when
/// it cannot be reached.
///
/// # Errors
///
/// Returns an error when the gateway cannot be started or attached, or
/// Docker fails.
pub(crate) fn http_status(service: &ServiceConfig, path: &str) -> Result<Option<u16>> {
    let (gateway, target) = prepare(service)?;
    let url = format!(
        "http://{target}:{}{path}",
        service.resolved_container_port()
    );
    let output = crate::docker::run_docker_output_owned(
        &[
            "exec".to_owned(),
            gateway,
            "wget".to_owned(),
            "-S".to_owned(),
            "-q".to_owned(),
            "-T".to_owned(),
            "2".to_owned(),
            "-O".to_owned(),
            "/dev/null".to_owned(),
            url,
        ],
        "failed to run a gateway health probe",
    )?;
    // busybox wget prints the response status line on stderr, also for
    // 4xx/5xx answers (when it exits non-zero).
    Ok(parse_status(&String::from_utf8_lossy(&output.stderr)))
}

/// Requests `path` from the service inside its network and returns the
/// output in the shape `curl -w '\n%{http_code}'` produces (body, then the
/// status on the last line), so HTTP health checks that inspect the body
/// work unchanged for services without a host port.
///
/// # Errors
///
/// Returns an error when the gateway cannot be started or attached, or
/// Docker fails.
pub(crate) fn http_probe_output(
    service: &ServiceConfig,
    path: &str,
) -> Result<std::process::Output> {
    use std::os::unix::process::ExitStatusExt;

    let (gateway, target) = prepare(service)?;
    let url = format!(
        "http://{target}:{}{path}",
        service.resolved_container_port()
    );
    let output = crate::docker::run_docker_output_owned(
        &[
            "exec".to_owned(),
            gateway,
            "wget".to_owned(),
            "-S".to_owned(),
            "-q".to_owned(),
            "-T".to_owned(),
            "2".to_owned(),
            "-O".to_owned(),
            "-".to_owned(),
            url,
        ],
        "failed to run a gateway health probe",
    )?;
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    Ok(match parse_status(&stderr) {
        Some(status) => {
            let mut stdout = output.stdout;
            stdout.extend_from_slice(format!("\n{status}").as_bytes());
            std::process::Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout,
                stderr: Vec::new(),
            }
        }
        None => std::process::Output {
            status: std::process::ExitStatus::from_raw(1 << 8),
            stdout: Vec::new(),
            stderr: stderr.into_bytes(),
        },
    })
}

fn parse_status(stderr: &str) -> Option<u16> {
    stderr
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("HTTP/")
                .and_then(|rest| rest.split_whitespace().nth(1))
                .and_then(|code| code.parse().ok())
        })
        .next_back()
}

/// Ensures the gateway runs and is attached to the service's network, and
/// returns `(gateway, container to dial)`.
fn prepare(service: &ServiceConfig) -> Result<(String, String)> {
    prepare_gateway(service).map_err(|error| {
        anyhow::Error::new(ProbeSetupError(format!(
            "cannot probe '{}' inside its network: {error:#}",
            service.name
        )))
    })
}

fn prepare_gateway(service: &ServiceConfig) -> Result<(String, String)> {
    let network = service.runtime.network.clone().ok_or_else(|| {
        anyhow::anyhow!(
            "service '{}' has no instance network to probe in",
            service.name
        )
    })?;
    let installation = crate::home::installation_id()?;
    let gateway = gateway_container_name(&installation);
    let attached = ATTACHED.get_or_init(Default::default);
    let known = attached
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .contains(&network);
    if !known && INSPECT_ONLY.load(std::sync::atomic::Ordering::Relaxed) {
        if !super::publish::is_attached(&network)? {
            anyhow::bail!(
                "the berth gateway is not attached to {network}; its health is unknown until {} \
                 starts the instance",
                crate::followup::command("up")
            );
        }
    } else if !known {
        let dirs = GatewayDirs::ensure()?;
        let _lock = crate::home::HostLock::acquire("gateway")?;
        let ports = GatewayPorts::current()?;
        super::publish::start_gateway(&installation, ports, &dirs)?;
        super::publish::attach_for_probes(&gateway, &network)?;
        attached
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(network);
    }
    Ok((gateway, service.container_name()?))
}

#[cfg(test)]
mod tests {
    use super::parse_status;

    #[test]
    fn the_last_status_line_wins_including_error_statuses() {
        let stderr = "Connecting to app:80\n  HTTP/1.1 301 Moved\n  HTTP/1.1 503 Service Unavailable\nwget: server returned error: HTTP/1.1 503";
        assert_eq!(parse_status(stderr), Some(503));
        assert_eq!(parse_status("wget: can't connect to remote host"), None);
    }
}
