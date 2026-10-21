//! A read-only query of a tenant's database, run by the server's admin.

use anyhow::Result;

use crate::config::ServiceConfig;

use crate::config::SharedEngine;

/// Runs berth's own `sql` (never a dump's) in `service`'s tenant database as
/// the shared server's admin, and returns its rows. A `docker exec` into the
/// running server: a throwaway client container costs seconds to start, and
/// the isolation it buys is for SQL berth did not write.
///
/// # Errors
///
/// Returns an error when the service is not shared, is Redis, or the query
/// fails.
pub(crate) fn tenant_query(service: &ServiceConfig, sql: &str) -> Result<String> {
    let target = service
        .runtime
        .shared
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("service '{}' is not shared", service.name))?;
    let database = service
        .database
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("service '{}' has no tenant database", service.name))?;
    let script = match target.engine {
        SharedEngine::Postgres => {
            format!("\\connect \"{}\"\n{sql}", database.replace('"', "\"\""))
        }
        SharedEngine::Mysql | SharedEngine::Mariadb => {
            format!("USE `{}`;\n{sql}", database.replace('`', "``"))
        }
        SharedEngine::Redis | SharedEngine::Valkey => {
            anyhow::bail!("service '{}' is not a SQL database", service.name)
        }
    };
    super::server::admin_sql(target, &script)
}
