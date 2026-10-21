//! Which of some tables an extension of the database owns.

use anyhow::{Context, Result};
use std::collections::BTreeSet;

use crate::config::ServiceConfig;

/// Marks the answer's rows in the client's output, which frames them.
const MARK: &str = "berth-extension-table:";

/// The `names` that are tables or sequences an extension of `service`'s
/// PostgreSQL database owns (its catalog, which a tenant may not write).
pub(super) fn extension_tables(
    service: &ServiceConfig,
    names: &BTreeSet<String>,
) -> Result<BTreeSet<String>> {
    if names.is_empty() {
        return Ok(BTreeSet::new());
    }
    let list = |qualified: bool| {
        let quoted = names
            .iter()
            .filter(|name| name.contains('.') == qualified)
            .map(|name| format!("'{}'", name.replace('\'', "''")))
            .collect::<Vec<_>>();
        // `IN ()` is invalid SQL; `NULL` matches nothing.
        if quoted.is_empty() {
            "NULL".to_owned()
        } else {
            quoted.join(", ")
        }
    };
    // A `schema.table` (a COPY's own table) names one relation; a bare name,
    // as psql's errors give it, counts only when no relation of that name is
    // the database's own. `relname` is a `name` (63 bytes), which would make
    // the union's column one too and cut a long `schema.table`: hence `text`.
    let query = format!(
        "SELECT '{MARK}' || k FROM (SELECT c.relname::text AS k, c.oid FROM pg_class c \
         WHERE c.relname IN ({bare}) UNION ALL SELECT n.nspname || '.' || c.relname, c.oid \
         FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname || '.' || c.relname IN ({qualified})) r GROUP BY k \
         HAVING bool_and(EXISTS (SELECT 1 FROM pg_depend d \
         WHERE d.classid = 'pg_class'::regclass AND d.objid = r.oid AND d.deptype = 'e'));\n",
        bare = list(false),
        qualified = list(true)
    );
    let rows = super::super::sql_admin::query(service, &query).with_context(|| {
        format!(
            "failed to ask '{}' which tables its extensions own",
            service.name
        )
    })?;
    Ok(marked(&rows))
}

/// The marked names in the client's output.
fn marked(stdout: &str) -> BTreeSet<String> {
    stdout
        .lines()
        .filter_map(|line| line.trim().strip_prefix(MARK))
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::marked;

    #[test]
    fn the_marked_rows_are_read_from_framed_output() {
        let stdout = "              ?column?\n-------------------------------------\n \
                      berth-extension-table:hypertable\n berth-extension-table:metadata\n(2 rows)\n";
        assert_eq!(
            marked(stdout).into_iter().collect::<Vec<_>>(),
            ["hypertable", "metadata"]
        );
    }
}
