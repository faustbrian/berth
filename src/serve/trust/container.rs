//! Container-side certificate discovery/copy helpers.

use anyhow::Result;
use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::config::ServiceConfig;

const CA_DISCOVERY_ATTEMPTS: usize = 3;
const CA_DISCOVERY_BACKOFF: Duration = Duration::from_secs(1);

/// Finds the inner Caddy root cert path, warming up TLS endpoint if needed.
pub(super) fn find_container_ca_with_warmup(
    target: &ServiceConfig,
    container_name: &str,
) -> Result<Option<String>> {
    for attempt in 0..CA_DISCOVERY_ATTEMPTS {
        let cert_path = find_container_ca_cert_path(container_name)?;
        if cert_path.is_some() {
            return Ok(cert_path);
        }
        if attempt + 1 == CA_DISCOVERY_ATTEMPTS {
            continue;
        }

        warm_up_container_tls_endpoint(target);
        thread::sleep(CA_DISCOVERY_BACKOFF);
    }

    Ok(None)
}

/// Copies the discovered container CA certificate to a host path.
pub(super) fn copy_container_ca_cert(
    container_name: &str,
    cert_path: &str,
    output_path: &str,
) -> Result<()> {
    let source = format!("{container_name}:{cert_path}");
    let output = crate::docker::run_docker_output(
        &["cp", &source, output_path],
        "failed to copy inner caddy root certificate from container",
    )?;
    super::command::ensure_output_success(
        output,
        &format!("failed to copy container CA certificate from '{cert_path}'"),
    )?;

    Ok(())
}

/// Locates a likely `root.crt` path inside the running container.
fn find_container_ca_cert_path(container_name: &str) -> Result<Option<String>> {
    let probe_script = r#"
for d in /data/caddy /data /var/lib/caddy /root/.local/share/caddy /home/*/.local/share/caddy; do
  if [ -f "$d/pki/authorities/local/root.crt" ]; then
    echo "$d/pki/authorities/local/root.crt"
    exit 0
  fi
done
find / -path '*/pki/authorities/local/root.crt' 2>/dev/null | head -n 1
"#;

    let output = crate::docker::run_docker_output(
        &["exec", container_name, "sh", "-lc", probe_script],
        "failed to probe container CA certificate path",
    )?;

    let output = match super::command::ensure_output_success(
        output,
        "failed to probe container CA certificate path",
    ) {
        Ok(output) => output,
        Err(_) => return Ok(None),
    };

    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if path.is_empty() {
        return Ok(None);
    }

    Ok(Some(path))
}

/// Sends a short TLS request to trigger lazy certificate materialization.
fn warm_up_container_tls_endpoint(target: &ServiceConfig) {
    if !crate::network::is_published(target) {
        return;
    }
    let url = format!("https://{}:{}", target.host, target.port);
    drop(
        Command::new("curl")
            .args(["-kfsSI", "--max-time", "5", &url])
            .output(),
    );
}
