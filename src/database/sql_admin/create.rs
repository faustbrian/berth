//! database sql admin create module.
//!
//! Contains database sql admin create logic used by Berth command workflows.

use anyhow::Result;

use crate::config::{Driver, ServiceConfig};

use super::super::setup::ensure_sql_service;
use super::common::{
    emit_sql_admin_dry_run, ensure_command_success, run_mysql_exec, run_postgres_exec, sql_context,
};

/// Creates database for downstream execution.
pub(super) fn create_database(service: &ServiceConfig) -> Result<()> {
    ensure_sql_service(service)?;

    if crate::docker::is_dry_run() {
        emit_sql_admin_dry_run(
            service,
            &format!(
                "[dry-run] Create database '{}' if missing",
                service.database.as_deref().unwrap_or("app")
            ),
        );
        return Ok(());
    }

    let ctx = sql_context(service)?;

    let output = create_database_output(&ctx)?;

    ensure_command_success(&output, "Failed to create database")?;

    Ok(())
}

fn create_database_output(ctx: &super::common::SqlContext) -> Result<std::process::Output> {
    match ctx.driver {
        Driver::Postgres => run_postgres_exec(
            ctx,
            "psql",
            &["-c".to_owned(), postgres_create_sql(&ctx.db_name)],
            "Failed to execute postgres CREATE DATABASE command",
        ),
        Driver::Mysql => run_mysql_exec(
            ctx,
            crate::docker::MysqlProgram::Client,
            &["-e".to_owned(), mysql_create_sql(&ctx.db_name)],
            "Failed to execute mysql CREATE DATABASE command",
        ),
        _ => unreachable!("validated SQL drivers in sql_context"),
    }
}

fn postgres_create_sql(db_name: &str) -> String {
    format!(
        "SELECT 'CREATE DATABASE {}' WHERE NOT EXISTS \
         (SELECT FROM pg_database WHERE datname = '{}')\\gexec",
        db_name, db_name
    )
}

fn mysql_create_sql(db_name: &str) -> String {
    format!("CREATE DATABASE IF NOT EXISTS `{db_name}`")
}
