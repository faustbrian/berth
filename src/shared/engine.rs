//! What differs between shared engines: how berth reaches the server as
//! its admin, and how the server is created.

use crate::config::SharedEngine;

use super::postgres;

/// The admin client command (run with the admin password in
/// [`password_env`]) and its handoff and authentication signals.
pub(super) fn admin_command(engine: SharedEngine, image: &str) -> Vec<&'static str> {
    match engine {
        SharedEngine::Postgres => vec![
            "psql",
            "--no-psqlrc",
            "-h",
            "127.0.0.1",
            "-v",
            "ON_ERROR_STOP=1",
            "-qtA",
            "-U",
            postgres::ADMIN_USER,
            "-d",
            "postgres",
        ],
        SharedEngine::Mysql | SharedEngine::Mariadb => vec![
            crate::docker::mysql_program(image, crate::docker::MysqlProgram::Client),
            "--protocol=tcp",
            "-h",
            "127.0.0.1",
            "-u",
            "root",
            "--batch",
            "--skip-column-names",
        ],
        // `-e` makes an error reply a non-zero exit; without it redis-cli
        // exits 0 after any reply.
        SharedEngine::Redis | SharedEngine::Valkey => vec!["redis-cli", "-h", "127.0.0.1", "-e"],
    }
}

/// The admin command that proves the server answers authenticated requests.
pub(super) fn ping(engine: SharedEngine) -> &'static str {
    match engine {
        SharedEngine::Postgres | SharedEngine::Mysql | SharedEngine::Mariadb => "SELECT 1;\n",
        SharedEngine::Redis | SharedEngine::Valkey => "PING\n",
    }
}

/// Server options berth adds after the image's command.
///
/// Connections: the images default to 100 (PostgreSQL) or 151 (MySQL,
/// MariaDB) connections, while one server serves every instance. Thirty
/// agents each running ParaTest `--parallel` (a process per core, about 8–16)
/// plus app, worker and Horizon connections need roughly 300–700, so
/// shared servers allow 1000.
///
/// Binary log: MySQL 8 enables it by default, and with it only `SUPER`
/// accounts may create triggers and stored routines (ERROR 1419), which
/// tenants are not; a development server needs no binary log, which would
/// also grow with every test schema the worktrees churn through. MariaDB
/// images keep it off.
///
/// Each option is skipped when an authored `command` already sets that
/// setting, so the project's own value wins.
pub(super) fn server_args(engine: SharedEngine, command: &[String]) -> Vec<String> {
    let options: &[(&str, &[&str])] = match engine {
        SharedEngine::Postgres => &[("max_connections", &["-c", "max_connections=1000"])],
        SharedEngine::Mysql => &[
            ("log_bin", &["--skip-log-bin"]),
            ("max_connections", &["--max-connections=1000"]),
        ],
        SharedEngine::Mariadb => &[("max_connections", &["--max-connections=1000"])],
        // Users live in an ACL file on the data volume (see `redis.rs`), so
        // tenants survive restarts and no password is a server argument.
        SharedEngine::Redis | SharedEngine::Valkey => {
            &[("aclfile", &["--aclfile", super::redis::ACL_FILE])]
        }
    };
    let authored = command.join(" ").replace('-', "_");
    options
        .iter()
        .filter(|(setting, _)| !authored.contains(setting))
        .flat_map(|(_, args)| args.iter().map(|arg| (*arg).to_owned()))
        .collect()
}

/// The client variable that carries the admin password.
pub(super) fn password_env(engine: SharedEngine) -> &'static str {
    match engine {
        SharedEngine::Postgres => "PGPASSWORD",
        SharedEngine::Mysql | SharedEngine::Mariadb => "MYSQL_PWD",
        SharedEngine::Redis | SharedEngine::Valkey => "REDISCLI_AUTH",
    }
}

/// The image variable that sets the admin password at initialization;
/// Redis and Valkey read it from their ACL file instead.
pub(super) fn server_password_env(engine: SharedEngine) -> Option<&'static str> {
    match engine {
        SharedEngine::Postgres => Some("POSTGRES_PASSWORD"),
        SharedEngine::Mysql | SharedEngine::Mariadb => Some("MYSQL_ROOT_PASSWORD"),
        SharedEngine::Redis | SharedEngine::Valkey => None,
    }
}

/// Whether a failed admin command could not reach the final server yet:
/// the images' temporary init server listens only on the local socket, so
/// TCP connections fail until it hands over (psql exit 2; MySQL client
/// errors 2002, 2003 and 2013). Redis and Valkey have no init server but
/// refuse connections until they listen and answer `LOADING` while they
/// read their data. `output` is the command's stdout and stderr.
pub(super) fn is_handoff(engine: SharedEngine, code: Option<i32>, output: &str) -> bool {
    let stderr = output;
    if is_auth_failure(engine, stderr) {
        return false;
    }
    match engine {
        // psql exits 2 for every connection failure, also permanent ones (a
        // missing role or database, a pg_hba rule), so only messages of a
        // server that is not up yet, or restarting, count.
        SharedEngine::Postgres => {
            code == Some(2)
                && [
                    "Connection refused",
                    "could not connect",
                    "the database system is starting up",
                    "the database system is shutting down",
                    "the database system is in recovery mode",
                    "the database system is not yet accepting connections",
                    "sorry, too many clients already",
                    "server closed the connection unexpectedly",
                ]
                .iter()
                .any(|error| stderr.contains(error))
        }
        SharedEngine::Mysql | SharedEngine::Mariadb => {
            code == Some(1)
                && ["ERROR 2002", "ERROR 2003", "ERROR 2013", "ERROR 1040"]
                    .iter()
                    .any(|error| stderr.contains(error))
        }
        SharedEngine::Redis | SharedEngine::Valkey => {
            code == Some(1)
                && [
                    "Could not connect",
                    "Connection refused",
                    "LOADING",
                    "max number of clients reached",
                ]
                .iter()
                .any(|error| stderr.contains(error))
        }
    }
}

/// Whether the engine, not the server, failed to run the admin command
/// (an overloaded or restarting engine): asking again may succeed.
pub(super) fn is_engine_exec_failure(output: &str) -> bool {
    crate::docker::is_engine_exec_failure(output)
}

/// Whether the server rejected the admin password.
pub(super) fn is_auth_failure(engine: SharedEngine, stderr: &str) -> bool {
    match engine {
        SharedEngine::Postgres => stderr.contains("authentication failed"),
        SharedEngine::Mysql | SharedEngine::Mariadb => stderr.contains("ERROR 1045"),
        SharedEngine::Redis | SharedEngine::Valkey => {
            stderr.contains("WRONGPASS") || stderr.contains("NOAUTH")
        }
    }
}

/// How to recover when the admin secret no longer matches the server,
/// without deleting the data every instance keeps there.
pub(super) fn lost_secret_recovery(engine: SharedEngine, server: &str) -> String {
    let secret = format!("~/.berth/shared/{server}/admin");
    match engine {
        SharedEngine::Postgres => format!(
            "reset it over the container's local socket: `printf \"ALTER ROLE {admin} \
             PASSWORD '%s';\" \"$(cat {secret})\" | docker exec -i {server} psql -U {admin} -d \
             postgres`",
            admin = postgres::ADMIN_USER
        ),
        SharedEngine::Mysql | SharedEngine::Mariadb => format!(
            "restore {secret} from a backup: MySQL does not trust the local socket, so the \
             password cannot be reset without it. Without a backup the only way forward is \
             removing the server and its volume (`docker rm -f -v {server} && docker volume \
             rm {server}-data`), which deletes every instance's shared databases"
        ),
        SharedEngine::Redis | SharedEngine::Valkey => format!(
            "restore {secret} from a backup. Without one, removing the server and its volume \
             (`docker rm -f -v {server} && docker volume rm {server}-data`) deletes only \
             caches, sessions and queued jobs, which every instance recreates"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{is_handoff, server_args};

    #[test]
    fn server_options_raise_limits_unless_the_project_sets_them() {
        assert_eq!(
            server_args(SharedEngine::Postgres, &[]),
            ["-c", "max_connections=1000"]
        );
        assert_eq!(
            server_args(SharedEngine::Mysql, &[]),
            ["--skip-log-bin", "--max-connections=1000"]
        );
        let tuned = [
            "postgres".to_owned(),
            "-c".to_owned(),
            "max_connections=300".to_owned(),
        ];
        assert!(server_args(SharedEngine::Postgres, &tuned).is_empty());
        let binlog = ["mysqld".to_owned(), "--log-bin=x".to_owned()];
        assert_eq!(
            server_args(SharedEngine::Mysql, &binlog),
            ["--max-connections=1000"]
        );
    }
    use crate::config::SharedEngine;

    #[test]
    fn only_connection_failures_are_handoffs() {
        assert!(is_handoff(
            SharedEngine::Postgres,
            Some(2),
            "could not connect"
        ));
        assert!(!is_handoff(
            SharedEngine::Postgres,
            Some(2),
            "password authentication failed"
        ));
        assert!(!is_handoff(SharedEngine::Postgres, Some(3), "syntax error"));
        for permanent in [
            "psql: error: connection to server at \"127.0.0.1\", port 5432 failed: FATAL:  role \"x\" does not exist",
            "FATAL:  database \"postgres\" does not exist",
            "FATAL:  no pg_hba.conf entry for host \"127.0.0.1\"",
        ] {
            assert!(
                !is_handoff(SharedEngine::Postgres, Some(2), permanent),
                "a connect-time FATAL never resolves by retrying: {permanent}"
            );
        }
        for starting in [
            "psql: error: connection to server at \"127.0.0.1\", port 5432 failed: Connection refused",
            "FATAL:  the database system is starting up",
            "FATAL:  the database system is shutting down",
            "server closed the connection unexpectedly",
            "FATAL:  the database system is in recovery mode",
            "FATAL:  the database system is not yet accepting connections",
            "FATAL:  sorry, too many clients already",
        ] {
            assert!(
                is_handoff(SharedEngine::Postgres, Some(2), starting),
                "{starting}"
            );
        }
        assert!(is_handoff(
            SharedEngine::Mysql,
            Some(1),
            "ERROR 2003 (HY000): Can't connect"
        ));
        assert!(!is_handoff(
            SharedEngine::Mariadb,
            Some(1),
            "ERROR 1045 (28000): Access denied"
        ));
        assert!(!is_handoff(
            SharedEngine::Mysql,
            Some(1),
            "ERROR 1064 (42000): syntax"
        ));
    }
}
