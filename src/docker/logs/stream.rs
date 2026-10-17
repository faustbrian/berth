//! docker logs stream module.
//!
//! Contains docker logs stream logic used by Berth command workflows.

use anyhow::{Context, Result};
use std::io::{BufRead, BufReader, Read};

use super::run::spawn_docker_stream;

pub(super) fn stream_logs_with_prefix(
    args: &[String],
    service_name: &str,
    container_name: &str,
    json: bool,
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
        for_each_line(stdout, |line| print_line(&out_name, "stdout", line, json));
    });

    let err_name = service_name.to_owned();
    let err_handle = std::thread::spawn(move || {
        for_each_line(stderr, |line| print_line(&err_name, "stderr", line, json));
    });

    let status = child.wait().context("Failed to wait on runtime logs")?;
    drop(out_handle.join());
    drop(err_handle.join());

    if !status.success() {
        anyhow::bail!("Failed to get logs for container '{container_name}'");
    }

    Ok(())
}

/// Prints one log line, as `[service] line` or as a JSON object.
fn print_line(service: &str, stream: &str, line: &str, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::json!({ "service": service, "stream": stream, "line": line })
        );
    } else {
        println!("[{service}] {line}");
    }
}

/// Calls `each` for every line of `source` until it ends. Log output is
/// bytes, not always UTF-8 (a database echoing Latin-1 query text, an app
/// writing raw bytes); invalid sequences become U+FFFD instead of ending the
/// stream, which would also leave `docker logs` writing into a closed pipe.
fn for_each_line(source: impl Read, mut each: impl FnMut(&str)) {
    let mut reader = BufReader::new(source);
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(_) => {
                let line = String::from_utf8_lossy(&buffer);
                each(line.trim_end_matches(['\n', '\r']));
            }
        }
    }
}
