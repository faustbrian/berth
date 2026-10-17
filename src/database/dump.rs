//! database dump module.
//!
//! Contains database dump logic used by Berth command workflows.

use anyhow::{Context, Result};
use std::path::Path;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::setup::ensure_sql_dump_service;
use super::sql_admin::{ensure_sql_command_success, run_dump_command};
use io::{is_gzip_path, write_dump_file, write_dump_stdout};

mod io;

pub(crate) fn dump(service: &ServiceConfig, file_path: &Path, gzip: bool) -> Result<()> {
    ensure_sql_dump_service(service)?;

    output::event(
        &service.name,
        LogLevel::Info,
        &dumping_to_file_message(service, file_path),
        Persistence::Persistent,
    );

    if crate::docker::is_dry_run() {
        emit_dump_dry_run(service, false);
        return Ok(());
    }

    let output = run_dump_command_checked(service, true)?;

    let use_gzip = gzip || is_gzip_path(file_path);
    write_dump_file(file_path, &output.stdout, use_gzip)?;

    let size = output.stdout.len();
    output::event(
        &service.name,
        LogLevel::Success,
        &dump_success_message(file_path, size),
        Persistence::Persistent,
    );
    Ok(())
}

pub(crate) fn dump_stdout(service: &ServiceConfig, gzip: bool) -> Result<()> {
    ensure_sql_dump_service(service)?;

    if crate::docker::is_dry_run() {
        emit_dump_dry_run(service, true);
        return Ok(());
    }

    let output = run_dump_command_checked(service, false)?;

    write_dump_stdout(&output.stdout, gzip).context("Failed to write dump to stdout")
}

fn dumping_to_file_message(service: &ServiceConfig, file_path: &Path) -> String {
    format!(
        "Dumping database '{}' to {}",
        service.database.as_deref().unwrap_or("app"),
        file_path.display()
    )
}

fn emit_dump_dry_run(service: &ServiceConfig, streaming: bool) {
    let message = dump_dry_run_message(service, streaming);
    output::event(
        &service.name,
        LogLevel::Info,
        &message,
        Persistence::Transient,
    );
}

fn run_dump_command_checked(
    service: &ServiceConfig,
    include_stderr: bool,
) -> Result<std::process::Output> {
    let output = run_dump_command(service)?;
    let failure_prefix = if include_stderr {
        "Failed to dump database"
    } else {
        "Failed to dump database command"
    };
    ensure_sql_command_success(&output, failure_prefix)?;
    Ok(output)
}

fn dump_success_message(file_path: &Path, size: usize) -> String {
    format!("Dumped {} ({} bytes raw)", file_path.display(), size)
}

fn dump_dry_run_message(service: &ServiceConfig, streaming: bool) -> String {
    if streaming {
        format!("[dry-run] Stream dump for '{}'", service.name)
    } else {
        format!("[dry-run] Dump database '{}'", service.name)
    }
}
