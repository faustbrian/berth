//! MySQL and MariaDB statements for shared servers and their tenants.
//!
//! Statements run as `root` over TCP inside the container, with the password
//! in `MYSQL_PWD` and SQL on stdin.

/// Quotes an identifier (`` `name` ``).
fn ident(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}

/// Quotes a string literal (`'value'`).
fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "''"))
}

/// A schema name as a `GRANT` pattern. In grants `_` and `%` are wildcards,
/// so a tenant named `shop__a` would otherwise also be granted `shopXXa`;
/// both are escaped.
fn grant_pattern(name: &str) -> String {
    name.replace('\\', "\\\\")
        .replace('_', "\\_")
        .replace('%', "\\%")
}

/// Creates or updates a tenant: a schema and a user with every privilege on
/// it and on `{schema}_test_%`, where ParaTest creates its databases. The
/// user can reach no other schema, so no tenant can create a name in
/// another's namespace.
#[must_use]
pub(crate) fn provision_tenant_sql(database: &str, user: &str, password: &str) -> String {
    let account = format!("{}@'%'", literal(user));
    let secret = literal(password);
    let pattern = grant_pattern(database);
    format!(
        "CREATE DATABASE IF NOT EXISTS {db};\n\
         CREATE USER IF NOT EXISTS {account} IDENTIFIED BY {secret};\n\
         ALTER USER {account} IDENTIFIED BY {secret};\n\
         GRANT ALL PRIVILEGES ON {own}.* TO {account};\n\
         GRANT ALL PRIVILEGES ON {tests}.* TO {account};\n",
        db = ident(database),
        own = ident(&pattern),
        tests = ident(&format!("{pattern}\\_test\\_%")),
    )
}

/// Lists every schema (`d\tname\t`) and user (`r\tname\t`), for drops to
/// filter. MySQL has no schema owners, so the owner column is empty.
#[must_use]
pub(crate) fn list_names_sql() -> String {
    "SELECT 'd', SCHEMA_NAME, '' FROM information_schema.SCHEMATA \
     UNION ALL SELECT 'r', User, '' FROM mysql.user;\n"
        .to_owned()
}

/// Drops tenant schemas and users.
#[must_use]
pub(crate) fn drop_sql(databases: &[String], users: &[String]) -> String {
    let mut sql = String::new();
    for database in databases {
        sql.push_str(&format!("DROP DATABASE IF EXISTS {};\n", ident(database)));
    }
    for user in users {
        sql.push_str(&format!("DROP USER IF EXISTS {}@'%';\n", literal(user)));
    }
    sql
}

#[cfg(test)]
mod tests {
    use super::{drop_sql, provision_tenant_sql};

    #[test]
    fn grants_cover_exactly_the_schema_and_its_test_databases() {
        let sql = provision_tenant_sql("shop__a-1__db", "0123456789ab_cdef01234567", "pa'ss");
        assert!(sql.contains("CREATE DATABASE IF NOT EXISTS `shop__a-1__db`;"));
        assert!(
            sql.contains("IDENTIFIED BY 'pa''ss'"),
            "literals are escaped"
        );
        assert!(
            sql.contains("ON `shop\\_\\_a-1\\_\\_db`.* TO"),
            "underscores are not wildcards: {sql}"
        );
        assert!(
            sql.contains("ON `shop\\_\\_a-1\\_\\_db\\_test\\_%`.* TO"),
            "{sql}"
        );
        assert!(!sql.contains("ON *.*"), "no global privilege");
    }

    #[test]
    fn drops_remove_schemas_and_users() {
        let sql = drop_sql(&["shop__a__db".to_owned()], &["u_1".to_owned()]);
        assert!(sql.contains("DROP DATABASE IF EXISTS `shop__a__db`;"));
        assert!(sql.contains("DROP USER IF EXISTS 'u_1'@'%';"));
    }
}
