//! database restore from file module.
//!
//! Contains database restore from file logic used by Berth command workflows.

use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::path::Path;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::RestoreOptions;
use super::process::start_restore_process;

const CHUNK_SIZE: usize = 8192;
const PROGRESS_PERCENT_STEP: u8 = 1;

pub(super) fn restore_from_file(
    service: &ServiceConfig,
    file_path: &Path,
    total_bytes: Option<u64>,
    reader: Box<dyn Read>,
    options: RestoreOptions,
) -> Result<()> {
    let (mut reader, statements) = super::process::tap(service, reader);
    output::event(
        &service.name,
        LogLevel::Info,
        &format!("Restoring database from {}", file_path.display()),
        Persistence::Persistent,
    );

    let mut restore = start_restore_process(service)?;

    let mut processed_bytes = 0_u64;
    let mut next_progress_log = PROGRESS_PERCENT_STEP;
    let mut buffer = [0_u8; CHUNK_SIZE];
    let stdin = &mut restore.stdin;
    let streamed = (|| -> Result<()> {
        loop {
            let bytes_read = reader.read(&mut buffer).with_context(|| {
                format!(
                    "failed to read {} (a damaged or truncated dump?)",
                    file_path.display()
                )
            })?;

            if bytes_read == 0 {
                break;
            }

            let chunk = buffer
                .get(..bytes_read)
                .ok_or_else(|| anyhow::anyhow!("Buffer slice out of bounds"))?;

            stdin
                .write_all(chunk)
                .context("Failed to write to database process")?;

            let Some(total_bytes) = total_bytes else {
                continue;
            };
            let chunk_len = u64::try_from(bytes_read).unwrap_or(0);
            processed_bytes = processed_bytes.saturating_add(chunk_len).min(total_bytes);
            let progress = progress_percent(processed_bytes, total_bytes);

            while progress >= next_progress_log {
                emit_progress_log(service, next_progress_log, processed_bytes, total_bytes);
                if next_progress_log == 100 {
                    break;
                }
                next_progress_log = next_progress_log
                    .saturating_add(PROGRESS_PERCENT_STEP)
                    .min(100);
            }
        }
        Ok(())
    })()
    .and_then(|()| restore.send_end());
    super::process::finish_restore(service, options, restore, streamed, &statements)?;
    output::event(
        &service.name,
        LogLevel::Success,
        "Database restored successfully",
        Persistence::Persistent,
    );
    Ok(())
}

fn emit_progress_log(service: &ServiceConfig, percent: u8, processed_bytes: u64, total_bytes: u64) {
    output::event(
        &service.name,
        LogLevel::Info,
        &format!(
            "Restore progress {percent}% ({:.2} MiB/{:.2} MiB)",
            bytes_to_mib(processed_bytes),
            bytes_to_mib(total_bytes)
        ),
        Persistence::Persistent,
    );
}

fn progress_percent(processed_bytes: u64, total_bytes: u64) -> u8 {
    if total_bytes == 0 {
        return 0;
    }
    let pct = processed_bytes.saturating_mul(100) / total_bytes;
    u8::try_from(pct.min(100)).unwrap_or(100)
}

fn bytes_to_mib(bytes: u64) -> f64 {
    const BYTES_PER_MIB: f64 = 1024.0 * 1024.0;
    bytes as f64 / BYTES_PER_MIB
}

/// Returns whether the path has a `.gz` extension.
pub(super) fn is_gzip_path(path: &Path) -> bool {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gz"))
}

#[cfg(test)]
mod tests {
    use super::progress_percent;

    #[test]
    fn progress_percent_clamps_and_handles_zero_total() {
        assert_eq!(progress_percent(0, 0), 0);
        assert_eq!(progress_percent(10, 100), 10);
        assert_eq!(progress_percent(199, 100), 100);
    }
}
