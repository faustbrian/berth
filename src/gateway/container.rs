//! The per-installation gateway container.

use anyhow::{Context, Result};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use super::GatewayPorts;

/// Pinned gateway image (Caddy 2.11.7 on Alpine). Alpine's busybox `wget` and
/// `nc` are used for in-network health probes. The Caddy binary carries the
/// `cap_net_bind_service` file capability, so the container must keep that
/// one capability even though it only listens on unprivileged ports
/// (8.x FI-41: without it, exec fails with "operation not permitted").
pub(crate) const GATEWAY_IMAGE: &str =
    "caddy@sha256:d8542f48d34a9cf4e4c11a478865229840e87e4c96ea3f439101f31a5d35f75f";

/// Label recording the gateway's run specification. Any change to it (image,
/// ports or run flags, via `SPEC_REVISION`) replaces a running gateway, so an
/// upgraded berth never keeps serving through an outdated container
/// (8.x FI-40's compatibility revision).
const SPEC_LABEL: &str = "dev.berth.gateway.spec";
/// Bump when the gateway's `docker run` flags change.
const SPEC_REVISION: u32 = 1;

/// Gateway state directories under the berth home.
pub(crate) struct GatewayDirs {
    /// Caddy data, including the persistent local CA (survives recreation).
    pub(crate) data: PathBuf,
    /// Generated configuration, mounted read-only.
    pub(crate) config: PathBuf,
}

impl GatewayDirs {
    /// Creates (if needed) and returns the gateway directories.
    ///
    /// # Errors
    ///
    /// Returns an error when a directory cannot be created.
    pub(crate) fn ensure() -> Result<Self> {
        Ok(Self {
            data: crate::home::private_dir(Path::new("gateway/data"))?,
            config: crate::home::private_dir(Path::new("gateway/config"))?,
        })
    }
}

/// The gateway container name for an installation.
#[must_use]
pub(crate) fn gateway_container_name(installation: &str) -> String {
    format!(
        "berth_gateway_{}",
        installation.get(..12).unwrap_or(installation)
    )
}

/// The gateway container name for the current installation.
///
/// # Errors
///
/// Returns an error when the installation ID cannot be read or created.
pub(crate) fn gateway_name() -> Result<String> {
    Ok(gateway_container_name(&crate::home::installation_id()?))
}

/// What `ensure_gateway` found or did.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum GatewayState {
    /// The gateway was already running with the right ports.
    Running,
    /// The gateway was created or started and loaded the config on start.
    Started,
}

/// Ensures the gateway runs with `ports`, creating or recreating it.
///
/// # Errors
///
/// Returns an error when a configured port is taken by another process or
/// Docker fails.
pub(crate) fn ensure_gateway(
    installation: &str,
    ports: GatewayPorts,
    dirs: &GatewayDirs,
) -> Result<GatewayState> {
    let name = gateway_container_name(installation);
    let expected_spec = format!(
        "r{SPEC_REVISION}:{}:{}:{}",
        ports.https,
        ports.http,
        GATEWAY_IMAGE.rsplit(':').next().unwrap_or(GATEWAY_IMAGE)
    );
    match inspect(&name)? {
        Some((true, labeled_spec)) if labeled_spec == expected_spec => {
            return Ok(GatewayState::Running);
        }
        // A stopped, created-but-never-started (for example after "port is
        // already allocated") or differently configured gateway is replaced;
        // its CA lives in the mounted data directory and survives.
        Some(_) => {
            docker(&["rm", "-f", &name], "failed to replace the berth gateway")?;
        }
        None => {}
    }

    for port in [ports.https, ports.http] {
        ensure_port_free(port)?;
    }
    let owner = owner_ids(&dirs.data)?;
    let https = format!("127.0.0.1:{0}:{0}", ports.https);
    let http = format!("127.0.0.1:{0}:{0}", ports.http);
    let data_mount = format!("{}:/data", dirs.data.display());
    let config_mount = format!("{}:/etc/berth:ro", dirs.config.display());
    let spec_label = format!("{SPEC_LABEL}={expected_spec}");
    let installation_label = format!("dev.berth.installation={installation}");
    docker(
        &[
            "run",
            "-d",
            "--name",
            &name,
            "--restart",
            "unless-stopped",
            "--label",
            "dev.berth.managed=true",
            "--label",
            &installation_label,
            "--label",
            "dev.berth.role=gateway",
            "--label",
            &spec_label,
            "--user",
            &owner,
            "--cap-drop",
            "ALL",
            "--cap-add",
            "NET_BIND_SERVICE",
            "--security-opt",
            "no-new-privileges",
            "--tmpfs",
            "/config:rw,noexec,nosuid,mode=1777,size=16m",
            "--tmpfs",
            "/tmp:rw,noexec,nosuid,mode=1777,size=16m",
            "-p",
            &https,
            "-p",
            &http,
            "-v",
            &data_mount,
            "-v",
            &config_mount,
            GATEWAY_IMAGE,
            "caddy",
            "run",
            "--config",
            "/etc/berth/caddy.json",
        ],
        "failed to start the berth gateway",
    )?;
    Ok(GatewayState::Started)
}

/// Whether the gateway container exists, in any state.
///
/// # Errors
///
/// Returns an error when Docker fails.
pub(crate) fn exists(name: &str) -> Result<bool> {
    Ok(inspect(name)?.is_some())
}

/// Whether the gateway exists and is running (not restarting or stopped).
///
/// # Errors
///
/// Returns an error when Docker fails.
pub(crate) fn is_running(name: &str) -> Result<bool> {
    Ok(inspect(name)?.is_some_and(|(running, _)| running))
}

/// Returns `(running, spec label)` for an existing gateway container. A
/// crash-looping gateway reports `restarting` and counts as not running, so
/// it is replaced.
fn inspect(name: &str) -> Result<Option<(bool, String)>> {
    let format = format!("{{{{.State.Status}}}} {{{{index .Config.Labels \"{SPEC_LABEL}\"}}}}");
    let output = crate::docker::run_docker_output_owned(
        &[
            "inspect".to_owned(),
            "--format".to_owned(),
            format,
            name.to_owned(),
        ],
        "failed to inspect the berth gateway",
    )?;
    if !output.status.success() {
        return Ok(None);
    }
    let rendered = String::from_utf8_lossy(&output.stdout);
    let mut fields = rendered.split_whitespace();
    let running = fields.next() == Some("running");
    let spec = fields.next().unwrap_or_default().to_owned();
    Ok(Some((running, spec)))
}

/// Fails, naming the listener when it can be identified, if `port` is taken.
/// Berth never falls back to another port (8.x FI-14).
fn ensure_port_free(port: u16) -> Result<()> {
    // `.localhost` clients may try `::1` first, so a listener there would
    // intercept berth's routes even though the gateway binds IPv4 (8.x FI-40).
    for address in ["127.0.0.1", "::1"] {
        let busy = match std::net::TcpListener::bind((address, port)) {
            Ok(_) => false,
            Err(error) => is_busy(address, &error)
                .with_context(|| format!("failed to check gateway port {address}:{port}"))?,
        };
        if busy {
            let owner = listener_owner(port);
            let shown = if address == "::1" {
                format!("[::1]:{port}")
            } else {
                format!("{address}:{port}")
            };
            match owner {
                Some(owner) => anyhow::bail!(
                    "gateway port {shown} is in use by '{owner}'; free it or set \
                     BERTH_GATEWAY_HTTPS_PORT / BERTH_GATEWAY_HTTP_PORT"
                ),
                None => anyhow::bail!(
                    "gateway port {shown} is in use; free it or set \
                     BERTH_GATEWAY_HTTPS_PORT / BERTH_GATEWAY_HTTP_PORT"
                ),
            }
        }
    }
    Ok(())
}

/// Classifies a failed bind: `AddrInUse` means another listener holds the
/// port. On `::1` any other error means this host has no IPv6 loopback (no
/// address, or the address family disabled as with `ipv6.disable=1`), so
/// nothing can listen there. On `127.0.0.1` any other error is unexpected.
fn is_busy(address: &str, error: &std::io::Error) -> std::io::Result<bool> {
    if error.kind() == std::io::ErrorKind::AddrInUse {
        return Ok(true);
    }
    if address == "::1" {
        return Ok(false);
    }
    Err(std::io::Error::new(error.kind(), error.to_string()))
}

fn listener_owner(port: u16) -> Option<String> {
    std::process::Command::new("lsof")
        .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN"])
        .output()
        .ok()
        .and_then(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .nth(1)
                .and_then(|line| line.split_whitespace().next().map(ToOwned::to_owned))
        })
}

/// The gateway runs as the user that owns the berth home, so files it writes
/// (the CA) stay owned by that user.
fn owner_ids(dir: &Path) -> Result<String> {
    let metadata =
        std::fs::metadata(dir).with_context(|| format!("failed to inspect {}", dir.display()))?;
    Ok(format!("{}:{}", metadata.uid(), metadata.gid()))
}

fn docker(args: &[&str], context: &str) -> Result<()> {
    let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
    let output = crate::docker::run_docker_output_owned(&args, context)?;
    if !output.status.success() {
        anyhow::bail!(
            "{context}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_busy;
    use std::io::{Error, ErrorKind};

    #[test]
    fn only_addr_in_use_is_busy_and_missing_ipv6_counts_as_free() {
        let in_use = Error::from(ErrorKind::AddrInUse);
        assert!(is_busy("127.0.0.1", &in_use).expect("ipv4"));
        assert!(is_busy("::1", &in_use).expect("ipv6"));
        // EAFNOSUPPORT (IPv6 disabled) is uncategorized; EADDRNOTAVAIL has no
        // `::1` address. Neither can hide a listener.
        for missing in [
            Error::from_raw_os_error(97),
            Error::from(ErrorKind::AddrNotAvailable),
        ] {
            assert!(!is_busy("::1", &missing).expect("no ipv6"));
        }
        assert!(is_busy("127.0.0.1", &Error::from(ErrorKind::PermissionDenied)).is_err());
    }
}
