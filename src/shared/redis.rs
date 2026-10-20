//! Redis and Valkey commands for shared servers and their tenants.
//!
//! A tenant is an ACL user confined to keys and channels under its prefix.
//! Commands run as the `default` user through `redis-cli` (both images ship
//! it) with the password in `REDISCLI_AUTH` and commands on stdin.

use sha2::{Digest, Sha256};

/// Where the server keeps its users, on its data volume.
pub(crate) const ACL_FILE: &str = "/data/users.acl";

/// The key and channel prefix of a tenant.
#[must_use]
pub(crate) fn prefix(user: &str) -> String {
    format!("{user}:")
}

fn password_hash(password: &str) -> String {
    Sha256::digest(password.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The ACL file a new server starts from: only the admin (`default`), whose
/// password is stored as its SHA-256 hash.
#[must_use]
pub(crate) fn initial_acl_file(admin_password: &str) -> String {
    format!(
        "user default on #{} ~* &* +@all\n",
        password_hash(admin_password)
    )
}

/// Creates or updates a tenant and saves the ACL file. The user reaches only
/// `{prefix}*` keys and channels; administrative and dangerous commands
/// (`KEYS`, `FLUSHDB`, `FLUSHALL`, `CONFIG`, `DEBUG`, …) are denied, so
/// `artisan cache:clear` needs `isolation: dedicated`. `SCAN` stays allowed
/// for Laravel and Horizon; it lists other tenants' key names, never values.
/// Server-wide script state stays out of reach: `SCRIPT FLUSH`, `SCRIPT KILL`
/// and `FUNCTION` (which Laravel does not use) are denied; `EVAL`,
/// `EVALSHA` and `SCRIPT LOAD` stay.
#[must_use]
pub(crate) fn provision_tenant_commands(user: &str, password: &str) -> String {
    let prefix = prefix(user);
    format!(
        "ACL SETUSER {user} reset on #{hash} ~{prefix}* &{prefix}* -@all +@read +@write \
         +@keyspace +@connection +@transaction +@pubsub +@scripting -@admin -@dangerous +scan \
         -script|flush -script|kill -function\n\
         ACL SAVE\n",
        hash = password_hash(password)
    )
}

/// Reads how many logical databases the server has.
#[must_use]
pub(crate) fn database_count_commands() -> String {
    "CONFIG GET databases\n".to_owned()
}

/// Parses `CONFIG GET databases` (`databases` then the count).
///
/// # Errors
///
/// Returns an error for any other reply.
pub(crate) fn parse_database_count(reply: &str) -> anyhow::Result<u32> {
    let mut lines = reply.lines().map(str::trim).filter(|line| !line.is_empty());
    match (lines.next(), lines.next().map(str::parse::<u32>)) {
        (Some("databases"), Some(Ok(count))) if count > 0 => Ok(count),
        _ => anyhow::bail!("unexpected CONFIG GET databases reply '{}'", reply.trim()),
    }
}

/// Reads the server's version (`INFO server`).
#[must_use]
pub(crate) fn server_info_commands() -> String {
    "INFO server\n".to_owned()
}

/// Fails unless the server is Redis 7 (or a Valkey, which reports a Redis 7
/// compatible `redis_version`): berth's admin commands need `redis-cli -e`
/// and tenants need ACL channel patterns.
///
/// # Errors
///
/// Returns an error for an older or unreadable version.
pub(crate) fn ensure_supported_version(info: &str) -> anyhow::Result<()> {
    let version = info
        .lines()
        .find_map(|line| line.trim().strip_prefix("redis_version:"))
        .ok_or_else(|| anyhow::anyhow!("INFO server reported no redis_version"))?;
    let major: u32 = version
        .split('.')
        .next()
        .and_then(|major| major.parse().ok())
        .ok_or_else(|| anyhow::anyhow!("unexpected redis_version '{version}'"))?;
    if major < 7 {
        anyhow::bail!(
            "shared Redis servers need Redis 7 or later (this image runs {version}); use a newer \
             `image:` tag, or set `isolation: dedicated` on the service"
        );
    }
    Ok(())
}

/// Lists ACL users, one per line.
#[must_use]
pub(crate) fn list_users_commands() -> String {
    "ACL USERS\n".to_owned()
}

/// Deletes `keys` (by logical database) and then `users`, and saves the
/// ACL file.
#[must_use]
pub(crate) fn drop_commands(keys: &[(u32, Vec<String>)], users: &[String]) -> String {
    let mut commands = String::new();
    for (database, found) in keys {
        commands.push_str(&format!("SELECT {database}\n"));
        for chunk in found.chunks(500) {
            commands.push_str("UNLINK");
            for key in chunk {
                commands.push(' ');
                commands.push_str(&quote(key));
            }
            commands.push('\n');
        }
    }
    for user in users {
        commands.push_str(&format!("ACL DELUSER {user}\n"));
    }
    commands.push_str("ACL SAVE\n");
    commands
}

/// Quotes a key for `redis-cli`'s line parser.
fn quote(key: &str) -> String {
    format!("\"{}\"", key.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::{drop_commands, initial_acl_file, provision_tenant_commands};

    #[test]
    fn tenants_reach_only_their_prefix_and_no_dangerous_command() {
        let commands = provision_tenant_commands("shop__a__cache", "secret");
        assert!(commands.contains("ACL SETUSER shop__a__cache reset on #"));
        assert!(
            commands.contains("~shop__a__cache:* &shop__a__cache:*"),
            "{commands}"
        );
        let rules = commands.lines().next().expect("rules");
        assert!(
            rules.find("-@dangerous") < rules.find("+scan"),
            "SCAN stays allowed after the deny"
        );
        assert!(!commands.contains("secret"), "only the hash is sent");
        assert!(commands.ends_with("ACL SAVE\n"));
    }

    #[test]
    fn the_admin_password_is_stored_as_a_hash() {
        let file = initial_acl_file("admin-secret");
        assert!(file.starts_with("user default on #"));
        assert!(!file.contains("admin-secret"));
    }

    #[test]
    fn drops_unlink_keys_in_each_database_before_deleting_users() {
        let keys = vec![
            (0, vec!["a:1".to_owned(), "a:\"q\"".to_owned()]),
            (1, vec!["a:cache".to_owned()]),
        ];
        let commands = drop_commands(&keys, &["a".to_owned()]);
        assert!(
            commands.starts_with(
                "SELECT 0\nUNLINK \"a:1\" \"a:\\\"q\\\"\"\nSELECT 1\nUNLINK \"a:cache\"\n"
            ),
            "{commands}"
        );
        assert!(commands.contains("ACL DELUSER a\nACL SAVE\n"));
    }

    #[test]
    fn databases_and_versions_are_read_precisely() {
        assert_eq!(
            super::parse_database_count("databases\n16\n").expect("count"),
            16
        );
        assert!(super::parse_database_count("ERR").is_err());
        assert!(super::ensure_supported_version("# Server\nredis_version:7.2.4\n").is_ok());
        assert!(super::ensure_supported_version("redis_version:6.2.14\n").is_err());
        assert!(super::ensure_supported_version("valkey_version:8.0.1\n").is_err());
    }

    #[test]
    fn server_wide_script_state_is_out_of_reach() {
        let commands = provision_tenant_commands("t", "p");
        assert!(
            commands.contains("-script|flush -script|kill -function"),
            "{commands}"
        );
    }
}
