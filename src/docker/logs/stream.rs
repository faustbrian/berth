//! docker logs stream module.
//!
//! Contains docker logs stream logic used by Berth command workflows.

use anyhow::{Context, Result};
use std::io::{BufRead, BufReader};

use super::run::spawn_docker_stream;

pub(super) fn stream_logs_with_prefix(
    args: &[String],
    service_name: &str,
    container_name: &str,
) -> Result<()> {
    let mut child =
        spawn_docker_stream(args, &crate::docker::runtime_command_error_context("logs"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to capture logs stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to capture logs stderr"))?;

    let out_name = service_name.to_owned();
    let out_handle = std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(std::result::Result::ok) {
            println!("[{out_name}] {line}");
        }
    });

    let err_name = service_name.to_owned();
    let err_handle = std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(std::result::Result::ok) {
            println!("[{err_name}] {line}");
        }
    });

    let status = child.wait().context("Failed to wait on runtime logs")?;
    drop(out_handle.join());
    drop(err_handle.join());

    if !status.success() {
        anyhow::bail!("Failed to get logs for container '{container_name}'");
    }

    Ok(())
}
