//! Whether a database already holds objects of its own (tables, types,
//! functions, schemas), asked as the service's user (a dedicated service)
//! or the shared server's admin (a tenant).

use anyhow::{Context, Result};

use crate::config::{Driver, ServiceConfig};

/// Whether `service`'s database holds any object of its own (a table,
/// sequence, type, function, schema, collation, operator, large object…;
/// see [`postgres_objects_query`]). A dry run assumes an empty database.
pub(super) fn has_tables(service: &ServiceConfig) -> Result<bool> {
    super::super::setup::ensure_sql_dump_service(service)?;
    if crate::docker::is_dry_run() {
        return Ok(false);
    }
    let postgres;
    let query = match service.driver {
        Driver::Postgres => {
            postgres = postgres_objects_query();
            postgres.as_str()
        }
        // Tables (views included), routines and events.
        Driver::Mysql => {
            "SELECT (SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = DATABASE()) \
             + (SELECT COUNT(*) FROM information_schema.routines \
             WHERE routine_schema = DATABASE()) \
             + (SELECT COUNT(*) FROM information_schema.events WHERE event_schema = DATABASE());\n"
        }
        _ => unreachable!("ensure_sql_dump_service admits only PostgreSQL and MySQL"),
    };
    let rows = super::super::sql_admin::query(service, query)
        .with_context(|| format!("failed to count the tables of '{}'", service.name))?;
    let count = parse_count(&rows)
        .with_context(|| format!("could not read the table count of '{}'", service.name))?;
    Ok(count > 0)
}

/// Catalogs of objects a schema holds, and their schema column: a seed that
/// failed partway can leave any of them (pg_dump writes collations, types
/// and functions before tables), and a later seed would run into it.
const NAMESPACED: [(&str, &str); 12] = [
    ("pg_proc", "pronamespace"),
    ("pg_collation", "collnamespace"),
    ("pg_conversion", "connamespace"),
    ("pg_operator", "oprnamespace"),
    ("pg_opclass", "opcnamespace"),
    ("pg_opfamily", "opfnamespace"),
    ("pg_ts_config", "cfgnamespace"),
    ("pg_ts_dict", "dictnamespace"),
    ("pg_ts_parser", "prsnamespace"),
    ("pg_ts_template", "tmplnamespace"),
    ("pg_statistic_ext", "stxnamespace"),
    ("pg_namespace", "oid"),
];

/// Objects of no schema a seed can make (`pg_subscription` is left out: a
/// tenant's seed cannot create one).
const UNNAMESPACED: [&str; 5] = [
    "pg_foreign_data_wrapper",
    "pg_foreign_server",
    "pg_publication",
    "pg_event_trigger",
    "pg_largeobject_metadata",
];

/// Counts what the database holds of its own: every object outside the
/// system schemas that no extension owns. The default image's timescaledb
/// (and PostGIS) put objects into every new database; an extension's
/// members carry a `pg_depend` row of deptype `e` on their own catalog (a
/// composite type's on its `pg_type` row, not its `pg_class` one, per
/// PostgreSQL's `heap.c`), and an index, array type or multirange follows
/// its owner (only types that are some type's `typarray` are skipped: a
/// domain over an array is the seed's own). A schema other than `public`
/// and an extension's home counts.
fn postgres_objects_query() -> String {
    let member = |catalog: &str, column: &str| {
        format!(
            "{column} NOT IN (SELECT objid FROM pg_depend WHERE deptype = 'e' \
             AND classid = '{catalog}'::regclass)"
        )
    };
    let own = "(SELECT oid FROM pg_namespace WHERE nspname NOT IN ('pg_catalog', \
               'information_schema') AND nspname !~ '^pg_')";
    let mut terms = vec![
        format!(
            "(SELECT count(*) FROM pg_class WHERE relnamespace IN {own} \
             AND relkind IN ('r', 'p', 'v', 'm', 'f', 'S', 'c') AND {} \
             AND (reltype = 0 OR {}))",
            member("pg_class", "oid"),
            member("pg_type", "reltype")
        ),
        format!(
            "(SELECT count(*) FROM pg_type WHERE typnamespace IN {own} AND typrelid = 0 \
             AND oid NOT IN (SELECT typarray FROM pg_type WHERE typarray <> 0) \
             AND typtype <> 'm' AND {})",
            member("pg_type", "oid")
        ),
    ];
    for (catalog, column) in NAMESPACED {
        // An extension's own schema (postgis_topology's `topology`) is its
        // home, not a member of it.
        let public = if catalog == "pg_namespace" {
            " AND nspname <> 'public' AND oid NOT IN (SELECT extnamespace FROM pg_extension)"
        } else {
            ""
        };
        terms.push(format!(
            "(SELECT count(*) FROM {catalog} WHERE {column} IN {own}{public} AND {})",
            member(catalog, "oid")
        ));
    }
    for catalog in UNNAMESPACED {
        terms.push(format!(
            "(SELECT count(*) FROM {catalog} WHERE {})",
            member(catalog, "oid")
        ));
    }
    format!("SELECT {};\n", terms.join(" + "))
}

/// The count in the query's output: its one line of digits.
fn parse_count(stdout: &str) -> Option<u64> {
    stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && line.bytes().all(|byte| byte.is_ascii_digit()))
        .and_then(|line| line.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::parse_count;

    #[test]
    fn the_count_is_read_from_either_clients_output() {
        assert_eq!(
            parse_count(" count \n-------\n     3\n(1 row)\n\n"),
            Some(3)
        );
        assert_eq!(parse_count("COUNT(*)\n0\n"), Some(0));
        assert_eq!(parse_count("nothing here\n"), None);
    }
}
