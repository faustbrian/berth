//! database restore preflight module.
//!
//! Contains shared restore preflight logic used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

use super::super::setup::ensure_sql_dump_service;
use super::super::sql_admin::reset_database;

pub(super) fn prepare_restore(service: &ServiceConfig, reset: bool) -> Result<()> {
    ensure_sql_dump_service(service)?;

    if reset {
        reset_database(service)?;
    }

    Ok(())
}
