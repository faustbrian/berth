//! database sql admin reset module.
//!
//! Contains database sql admin reset logic used by Berth command workflows.

use anyhow::Result;

use crate::config::{Driver, ServiceConfig};
use crate::output::{self, LogLevel, Persistence};

use super::super::setup::ensure_sql_dump_service;
use super::common::{
    SqlContext, emit_sql_admin_dry_run, ensure_command_success, run_mysql_exec, run_postgres_exec,
    sql_context,
};

pub(super) fn reset_database(service: &ServiceConfig) -> Result<()> {
    ensure_sql_dump_service(service)?;

    if crate::docker::is_dry_run() {
        emit_sql_admin_dry_run(
            service,
            &format!("[dry-run] Reset database for '{}'", service.name),
        );
        return Ok(());
    }

    let ctx = sql_context(service)?;

    output::event(
        &service.name,
        LogLevel::Info,
        &format!("Resetting database '{}'", ctx.db_name),
        Persistence::Persistent,
    );

    let (drop_sql, create_sql) = reset_sql(&ctx);
    run_sql_admin(&ctx, &drop_sql, "drop")?;
    run_sql_admin(&ctx, &create_sql, "create")?;

    output::event(
        &service.name,
        LogLevel::Success,
        &format!("Database '{}' reset", ctx.db_name),
        Persistence::Persistent,
    );
    Ok(())
}

fn reset_sql(ctx: &SqlContext) -> (String, String) {
    match ctx.driver {
        Driver::Postgres => (
            format!("DROP DATABASE IF EXISTS \"{}\"", ctx.db_name),
            format!("CREATE DATABASE \"{}\"", ctx.db_name),
        ),
        Driver::Mysql => (
            format!("DROP DATABASE IF EXISTS `{}`", ctx.db_name),
            format!("CREATE DATABASE `{}`", ctx.db_name),
        ),
        _ => unreachable!("validated SQL drivers in sql_context"),
    }
}

/// Runs a privileged SQL admin command (`drop`/`create`) inside the DB container.
fn run_sql_admin(ctx: &SqlContext, sql: &str, action: &str) -> Result<()> {
    let context = format!("Failed to {action} database");
    let output = match ctx.driver {
        Driver::Postgres => run_postgres_exec(
            ctx,
            "psql",
            &[
                "-d".to_owned(),
                "postgres".to_owned(),
                "-c".to_owned(),
                sql.to_owned(),
            ],
            &context,
        )?,
        Driver::Mysql => {
            run_mysql_exec(ctx, "mysql", &["-e".to_owned(), sql.to_owned()], &context)?
        }
        _ => unreachable!("validated SQL drivers in sql_context"),
    };

    ensure_command_success(&output, &format!("Failed to {action} database"))
}
