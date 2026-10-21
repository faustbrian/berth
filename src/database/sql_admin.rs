//! database sql admin module.
//!
//! Contains database sql admin logic used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

mod common;
mod create;
mod dump;
mod query;
mod reset;

/// Creates database for downstream execution.
///
/// A shared-backed service's database is its tenant, which berth provisions
/// (closed to other tenants) rather than a bare `CREATE DATABASE`.
pub(super) fn create_database(service: &ServiceConfig) -> Result<()> {
    if service.runtime.shared.is_some() {
        return crate::shared::up(service);
    }
    create::create_database(service)
}

/// Empties the service's database. A shared-backed service's tenant is
/// dropped and provisioned again, so the new database stays closed to other
/// tenants.
pub(super) fn reset_database(service: &ServiceConfig) -> Result<()> {
    if service.runtime.shared.is_some() {
        crate::shared::drop_tenant(service)?;
        return crate::shared::up(service);
    }
    reset::reset_database(service)
}

/// Executes dump command for the selected database service.
pub(super) fn dump_command_args(service: &ServiceConfig) -> Result<Vec<String>> {
    dump::dump_command_args(service)
}

pub(crate) fn ensure_sql_command_success(
    output: &std::process::Output,
    failure_prefix: &str,
) -> Result<()> {
    common::ensure_command_success(output, failure_prefix)
}

/// Runs berth's own read-only `sql` in the service's database.
pub(crate) fn query(service: &ServiceConfig, sql: &str) -> Result<String> {
    query::query(service, sql)
}
