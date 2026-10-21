//! database dump module.
//!
//! Contains database dump logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::setup::ensure_sql_dump_service;
use super::sql_admin::dump_command_args;
use io::{is_gzip_path, write_dump_file, write_dump_stdout};

mod io;
mod stream;

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

    let args = dump_command_args(service)?;
    let use_gzip = gzip || is_gzip_path(file_path);
    let size = write_dump_file(file_path, use_gzip, |sink| {
        stream::stream_dump(&args, sink, &dump_failure(service))
    })?;
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

    let args = dump_command_args(service)?;
    write_dump_stdout(gzip, |sink| {
        stream::stream_dump(&args, sink, &dump_failure(service))
    })
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

fn dump_failure(service: &ServiceConfig) -> String {
    format!("Failed to dump the database of service '{}'", service.name)
}

fn dump_success_message(file_path: &Path, size: u64) -> String {
    format!("Dumped {} ({} bytes raw)", file_path.display(), size)
}

fn dump_dry_run_message(service: &ServiceConfig, streaming: bool) -> String {
    if streaming {
        format!("[dry-run] Stream dump for '{}'", service.name)
    } else {
        format!("[dry-run] Dump database '{}'", service.name)
    }
}
