//! A read-only query of a service's database, by berth (never a dump).

use anyhow::Result;

use crate::config::{Driver, ServiceConfig};

use super::common::{ensure_command_success, run_mysql_exec, run_postgres_exec, sql_context};

/// Runs berth's own `sql` in `service`'s database and returns its rows, one
/// per line without headers: a `docker exec` into the running container,
/// or the shared server's admin for a tenant.
pub(super) fn query(service: &ServiceConfig, sql: &str) -> Result<String> {
    if service.runtime.shared.is_some() {
        return crate::shared::tenant_query(service, sql);
    }
    let ctx = sql_context(service)?;
    let context = format!("failed to query the database of '{}'", service.name);
    let output = match ctx.driver {
        Driver::Postgres => run_postgres_exec(
            &ctx,
            "psql",
            &[
                "--no-psqlrc".to_owned(),
                "-v".to_owned(),
                "ON_ERROR_STOP=1".to_owned(),
                "-qtA".to_owned(),
                "-d".to_owned(),
                ctx.db_name.clone(),
                "-c".to_owned(),
                sql.to_owned(),
            ],
            &context,
        )?,
        Driver::Mysql => run_mysql_exec(
            &ctx,
            crate::docker::MysqlProgram::Client,
            &[
                "--batch".to_owned(),
                "--skip-column-names".to_owned(),
                ctx.db_name.clone(),
                "-e".to_owned(),
                sql.to_owned(),
            ],
            &context,
        )?,
        _ => unreachable!("validated SQL drivers in sql_context"),
    };
    ensure_command_success(&output, &context)?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
