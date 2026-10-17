//! database sql admin dump module.
//!
//! Contains database sql admin dump logic used by Berth command workflows.

use anyhow::Result;

use crate::config::{Driver, ServiceConfig};

use super::super::setup::ensure_sql_dump_service;
use super::common::{run_mysql_exec, run_postgres_exec, sql_context};

/// Executes dump command for the selected database service.
pub(super) fn run_dump_command(service: &ServiceConfig) -> Result<std::process::Output> {
    ensure_sql_dump_service(service)?;

    let ctx = sql_context(service)?;

    match ctx.driver {
        Driver::Postgres => run_postgres_exec(
            &ctx,
            "pg_dump",
            std::slice::from_ref(&ctx.db_name),
            "Failed to execute pg_dump",
        ),
        // `--no-tablespaces`: dumping tablespaces needs the global PROCESS
        // privilege, which a shared server's tenant does not have.
        Driver::Mysql => run_mysql_exec(
            &ctx,
            crate::docker::MysqlProgram::Dump,
            &["--no-tablespaces".to_owned(), ctx.db_name.clone()],
            "Failed to execute mysqldump",
        ),
        _ => unreachable!("validated SQL drivers in sql_context"),
    }
}
