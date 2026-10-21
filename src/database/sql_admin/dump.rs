//! database sql admin dump module.
//!
//! Contains database sql admin dump logic used by Berth command workflows.

use anyhow::Result;

use crate::config::{Driver, ServiceConfig};

use super::super::setup::ensure_sql_dump_service;
use super::common::{mysql_exec_args, postgres_exec_args, sql_context};

/// The `docker` arguments that write the service's database to stdout.
pub(super) fn dump_command_args(service: &ServiceConfig) -> Result<Vec<String>> {
    ensure_sql_dump_service(service)?;
    if service.runtime.shared.is_some() {
        return crate::docker::tenant_dump_args(service);
    }

    let ctx = sql_context(service)?;

    match ctx.driver {
        Driver::Postgres => Ok(postgres_exec_args(
            &ctx,
            "pg_dump",
            &postgres_dump_args(&ctx.db_name),
        )),
        // `--no-tablespaces`: dumping tablespaces needs the global PROCESS
        // privilege, which a shared server's tenant does not have.
        Driver::Mysql => Ok(mysql_exec_args(
            &ctx,
            crate::docker::MysqlProgram::Dump,
            &["--no-tablespaces".to_owned(), ctx.db_name.clone()],
        )),
        _ => unreachable!("validated SQL drivers in sql_context"),
    }
}

/// `pg_dump`'s arguments. `--no-owner --no-privileges`: a dump that names
/// its roles fails to restore into another instance, whose tenant is a role
/// of its own.
fn postgres_dump_args(database: &str) -> Vec<String> {
    vec![
        "--no-owner".to_owned(),
        "--no-privileges".to_owned(),
        database.to_owned(),
    ]
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_postgres_dump_names_no_owners_or_grants() {
        assert_eq!(
            super::postgres_dump_args("app"),
            ["--no-owner", "--no-privileges", "app"]
        );
    }
}
