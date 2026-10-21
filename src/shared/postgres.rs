//! PostgreSQL statements for shared servers and their tenants.
//!
//! Statements run as the server's admin over the container's local socket,
//! with SQL on stdin, so no secret ever appears in a process argument.

/// The admin role berth creates every shared PostgreSQL server with.
pub(crate) const ADMIN_USER: &str = "berth_admin";

/// Quotes an identifier (`"name"`).
fn ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Quotes a string literal (`'value'`).
fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Oldest server berth shares: tenant drops need `DROP DATABASE ... WITH
/// (FORCE)`, added in PostgreSQL 13.
const MIN_VERSION_NUM: u32 = 130_000;

/// Reads `server_version_num`.
#[must_use]
pub(crate) fn server_version_sql() -> String {
    "SHOW server_version_num;\n".to_owned()
}

/// Fails unless the server is new enough to share.
///
/// # Errors
///
/// Returns an error for an unparsable or older version.
pub(crate) fn ensure_supported_version(version_num: &str) -> anyhow::Result<()> {
    let version: u32 = version_num
        .parse()
        .map_err(|_| anyhow::anyhow!("unexpected PostgreSQL server_version_num '{version_num}'"))?;
    if version < MIN_VERSION_NUM {
        anyhow::bail!(
            "shared PostgreSQL servers need PostgreSQL 13 or later (this image runs {}); use a \
             newer `image:` tag, or set `isolation: dedicated` on the service",
            postgres_version_text(version)
        );
    }
    Ok(())
}

/// Hardening of a server, run on every readying (idempotent). Nobody but
/// owners may create objects in the `postgres` database's `public` schema,
/// and only the admin may connect to `template1`, so no tenant can plant
/// objects that later databases inherit. `template1`'s own `public` schema
/// keeps its defaults: before PostgreSQL 15 it grants `CREATE` to `PUBLIC`,
/// and the tenant databases copied from it rely on that (their `public`
/// schema belongs to the admin there), while `REVOKE CONNECT` on each tenant
/// database keeps other tenants out of it.
#[must_use]
pub(crate) fn harden_server_sql() -> String {
    "REVOKE CREATE ON SCHEMA public FROM PUBLIC;\nREVOKE CONNECT ON DATABASE template1 FROM PUBLIC;\n".to_owned()
}

/// SQL that is true when `column` names something in the namespace of
/// `base` (`base` itself or `base_...`).
fn in_namespace(column: &str, base: &str) -> String {
    let base = literal(base);
    format!("({column} = {base} OR left({column}, length({base}) + 1) = {base} || '_')")
}

/// Creates or updates a tenant: a login role and a database it owns,
/// closed to everyone else. The role may create databases so ParaTest can
/// make `{tenant}_test_N`. Any tenant may therefore create a database with a
/// name in another instance's namespace (`base`); such a squatted database
/// is dropped here, before the namespace's own tenant uses it, so it never
/// receives a neighbour's data.
#[must_use]
pub(crate) fn provision_tenant_sql(tenant: &str, password: &str, base: &str) -> String {
    let role = ident(tenant);
    let name = literal(tenant);
    let secret = literal(password);
    let squatted = format!(
        "SELECT 'DROP DATABASE ' || quote_ident(datname) || ' WITH (FORCE)' FROM pg_database \
         WHERE {} AND NOT {}\\gexec\n",
        in_namespace("datname", base),
        in_namespace("pg_get_userbyid(datdba)", base)
    );
    let options = "LOGIN CREATEDB NOSUPERUSER NOCREATEROLE NOREPLICATION NOBYPASSRLS";
    format!(
        "DO $berth$ BEGIN\n\
         IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = {name}) THEN\n\
         CREATE ROLE {role} {options} PASSWORD {secret};\n\
         ELSE\n\
         ALTER ROLE {role} {options} PASSWORD {secret};\n\
         END IF;\n\
         END $berth$;\n\
         ALTER ROLE {role} SET lc_messages = 'C';\n\
         {squatted}\
         SELECT 'CREATE DATABASE ' || quote_ident({name}) || ' OWNER ' || quote_ident({name}) \
         WHERE NOT EXISTS (SELECT 1 FROM pg_database WHERE datname = {name})\\gexec\n\
         REVOKE ALL ON DATABASE {role} FROM PUBLIC;\n\
         GRANT CONNECT, TEMPORARY ON DATABASE {role} TO {role};\n"
    )
}

/// Lists every database with its owner (`d\tname\towner`) and every role
/// (`r\tname\t`), for drops to filter.
#[must_use]
pub(crate) fn list_names_sql() -> String {
    "SELECT 'd' || chr(9) || datname || chr(9) || pg_get_userbyid(datdba) FROM pg_database \
     UNION ALL SELECT 'r' || chr(9) || rolname || chr(9) FROM pg_roles;\n"
        .to_owned()
}

/// Drops tenant databases (closing their sessions) and then roles.
#[must_use]
pub(crate) fn drop_sql(databases: &[String], roles: &[String]) -> String {
    let mut sql = String::new();
    for database in databases {
        sql.push_str(&format!(
            "DROP DATABASE IF EXISTS {} WITH (FORCE);\n",
            ident(database)
        ));
    }
    for role in roles {
        sql.push_str(&format!("DROP ROLE IF EXISTS {};\n", ident(role)));
    }
    sql
}

/// `server_version_num` as people write it: `MMmmpp` before PostgreSQL 10
/// (90624 is 9.6.24), `MM00mm` from 10 on (160002 is 16.2).
fn postgres_version_text(version: u32) -> String {
    if version < 100_000 {
        format!(
            "{}.{}.{}",
            version / 10_000,
            version / 100 % 100,
            version % 100
        )
    } else {
        format!("{}.{}", version / 10_000, version % 10_000)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn versions_print_as_released() {
        assert_eq!(super::postgres_version_text(90_624), "9.6.24");
        assert_eq!(super::postgres_version_text(120_017), "12.17");
    }

    use super::{drop_sql, ensure_supported_version, provision_tenant_sql};

    #[test]
    fn hardening_closes_template1_without_taking_tenants_their_schema() {
        let sql = super::harden_server_sql();
        assert!(sql.contains("REVOKE CONNECT ON DATABASE template1 FROM PUBLIC"));
        // Before PostgreSQL 15, databases copied from template1 rely on its
        // public schema granting CREATE.
        assert!(!sql.contains("\\connect template1"), "{sql}");
    }

    #[test]
    fn servers_before_13_are_refused() {
        assert!(ensure_supported_version("180001").is_ok());
        assert!(ensure_supported_version("130000").is_ok());
        assert!(ensure_supported_version("120017").is_err());
        assert!(ensure_supported_version("").is_err());
    }

    #[test]
    fn tenants_are_unprivileged_owners_of_a_closed_database() {
        let sql = provision_tenant_sql("shop__a-1__db", "pa'ss", "shop__a-1");
        assert!(
            sql.contains("CREATE ROLE \"shop__a-1__db\" LOGIN CREATEDB NOSUPERUSER NOCREATEROLE")
        );
        assert!(sql.contains("PASSWORD 'pa''ss'"), "literals are escaped");
        let squat = sql
            .find("left(pg_get_userbyid(datdba)")
            .expect("squatted databases are dropped");
        assert!(
            squat < sql.find("SELECT 'CREATE DATABASE").expect("create"),
            "before the tenant's own"
        );
        assert!(sql.contains("REVOKE ALL ON DATABASE \"shop__a-1__db\" FROM PUBLIC"));
        // A restore judges the server's errors by their English wording.
        assert!(sql.contains("ALTER ROLE \"shop__a-1__db\" SET lc_messages = 'C';"));
        assert!(!sql.contains("SUPERUSER PASSWORD"), "{sql}");
    }

    #[test]
    fn drops_close_sessions_and_remove_roles_last() {
        let sql = drop_sql(&["shop__a".to_owned()], &["shop__a".to_owned()]);
        let database = sql
            .find("DROP DATABASE IF EXISTS \"shop__a\" WITH (FORCE)")
            .expect("db");
        let role = sql.find("DROP ROLE IF EXISTS \"shop__a\"").expect("role");
        assert!(database < role);
    }
}
