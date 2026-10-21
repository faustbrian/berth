//! docker exec args module.
//!
//! Contains docker exec args logic used by Berth command workflows.

use anyhow::Result;

use crate::config::{Driver, ServiceConfig};

/// Builds exec args for command execution.
pub(crate) fn build_exec_args(container_name: &str, command: &[String], tty: bool) -> Vec<String> {
    let mut args = vec![
        "exec".to_owned(),
        if tty { "-it" } else { "-i" }.to_owned(),
        container_name.to_owned(),
    ];
    args.extend(command.iter().cloned());
    args
}

/// Builds exec args for a database client: `env` (`NAME=value`) is set in
/// the client's environment, so a password passed as `MYSQL_PWD` never
/// becomes a client argument (and berth moves its value out of docker's
/// argv).
pub(crate) fn build_client_exec_args(
    container_name: &str,
    env: &[String],
    command: &[String],
    tty: bool,
) -> Vec<String> {
    let mut args = vec!["exec".to_owned(), if tty { "-it" } else { "-i" }.to_owned()];
    for pair in env {
        args.extend(["-e".to_owned(), pair.clone()]);
    }
    args.push(container_name.to_owned());
    args.extend(command.iter().cloned());
    args
}

/// The environment a database client needs: MySQL-family clients read the
/// password from `MYSQL_PWD`.
pub(super) fn client_env(service: &ServiceConfig) -> Vec<String> {
    match service.driver {
        Driver::Mysql => vec![format!(
            "MYSQL_PWD={}",
            service.password.as_deref().unwrap_or("secret")
        )],
        Driver::Redis | Driver::Valkey | Driver::Dragonfly => service
            .password
            .iter()
            .map(|password| format!("REDISCLI_AUTH={password}"))
            .collect(),
        _ => Vec::new(),
    }
}

/// [`client_env`] for the piped client a restore streams into. psql's
/// errors are read, so they must be English: the server's messages follow
/// `lc_messages`, which the dedicated server's superuser sets per session.
pub(super) fn piped_client_env(service: &ServiceConfig) -> Vec<String> {
    let mut env = client_env(service);
    if service.driver == Driver::Postgres {
        // A restore reads the server's errors, so it asks for English ones.
        // Only a superuser may set `lc_messages`: a dedicated service's user
        // is one (the image creates `POSTGRES_USER` as superuser), and a
        // database that refuses it fails the reachability probe before a
        // restore changes anything (`ensure_client_reachable`).
        // psql lexes the dump by the session's `standard_conforming_strings`,
        // and berth's line tap follows it with the setting on: the session
        // starts on, whatever the server's configuration.
        env.push("PGOPTIONS=-c lc_messages=C -c standard_conforming_strings=on".to_owned());
        // psql's own errors (`psql:<stdin>:3: error: …`) follow the client's
        // locale, and an image's `LC_ALL` (timescaledb-ha sets one) outranks
        // `LC_MESSAGES`. Reading a dump from stdin, psql takes its encoding
        // from the connection, not the locale.
        env.push("LC_ALL=C".to_owned());
    }
    env
}

pub(super) fn interactive_client_args(service: &ServiceConfig) -> Vec<String> {
    match service.driver {
        Driver::Postgres => vec![
            "psql".to_owned(),
            "-U".to_owned(),
            service
                .username
                .clone()
                .unwrap_or_else(|| "postgres".to_owned()),
            service.database.clone().unwrap_or_else(|| "app".to_owned()),
        ],
        Driver::Mysql => {
            [
                crate::docker::mysql_program(&service.image, crate::docker::MysqlProgram::Client)
                    .to_owned(),
            ]
            .into_iter()
            .chain(crate::docker::MYSQL_LOCAL_TCP.map(ToOwned::to_owned))
            .chain([
                "-u".to_owned(),
                service
                    .username
                    .clone()
                    .unwrap_or_else(|| "root".to_owned()),
                service.database.clone().unwrap_or_else(|| "app".to_owned()),
            ])
            .collect()
        }
        Driver::Redis | Driver::Valkey | Driver::Dragonfly => {
            let mut command = vec![
                "redis-cli".to_owned(),
                "-h".to_owned(),
                "127.0.0.1".to_owned(),
                "-p".to_owned(),
                service.default_port().to_string(),
            ];
            // An ACL user (a shared server's tenant) authenticates by name;
            // the password comes from `REDISCLI_AUTH` (see `client_env`).
            if let Some(user) = &service.username {
                command.extend(["--user".to_owned(), user.clone()]);
            }
            command
        }
        Driver::Sqlserver => vec![
            "sqlcmd".to_owned(),
            "-S".to_owned(),
            "localhost".to_owned(),
            "-U".to_owned(),
            service.username.clone().unwrap_or_else(|| "sa".to_owned()),
            "-P".to_owned(),
            service
                .password
                .clone()
                .unwrap_or_else(|| "BerthSqlServerPassw0rd!".to_owned()),
        ],
        Driver::Mongodb
        | Driver::Memcached
        | Driver::Garage
        | Driver::Rustfs
        | Driver::Localstack
        | Driver::Opensearch
        | Driver::Elasticsearch
        | Driver::Meilisearch
        | Driver::Typesense
        | Driver::Frankenphp
        | Driver::Reverb
        | Driver::Horizon
        | Driver::Scheduler
        | Driver::Dusk
        | Driver::Gotenberg
        | Driver::Mailhog
        | Driver::Rabbitmq
        | Driver::Soketi => {
            vec!["sh".to_owned()]
        }
    }
}

pub(super) fn piped_client_args(service: &ServiceConfig) -> Result<Vec<String>> {
    match service.driver {
        // psql runs the whole dump and exits 0 whatever failed: the restore
        // reads its errors and berth's markers (`psql_restore.rs`).
        Driver::Postgres => Ok(super::psql_restore::psql_restore_args()
            .into_iter()
            .chain([
                "-U".to_owned(),
                service
                    .username
                    .clone()
                    .unwrap_or_else(|| "postgres".to_owned()),
                service.database.clone().unwrap_or_else(|| "app".to_owned()),
            ])
            .collect()),
        Driver::Mysql => {
            Ok([
                crate::docker::mysql_program(&service.image, crate::docker::MysqlProgram::Client)
                    .to_owned(),
            ]
            .into_iter()
            .chain(crate::docker::MYSQL_LOCAL_TCP.map(ToOwned::to_owned))
            .chain([
                // Batch output without column names: berth reads its own
                // rows after the dump (`database/restore/process.rs`).
                "--skip-column-names".to_owned(),
                "-u".to_owned(),
                service
                    .username
                    .clone()
                    .unwrap_or_else(|| "root".to_owned()),
                service.database.clone().unwrap_or_else(|| "app".to_owned()),
            ])
            .collect())
        }
        Driver::Sqlserver => Ok(vec![
            "sqlcmd".to_owned(),
            "-S".to_owned(),
            "localhost".to_owned(),
            "-U".to_owned(),
            service.username.clone().unwrap_or_else(|| "sa".to_owned()),
            "-P".to_owned(),
            service
                .password
                .clone()
                .unwrap_or_else(|| "BerthSqlServerPassw0rd!".to_owned()),
            "-d".to_owned(),
            service
                .database
                .clone()
                .unwrap_or_else(|| "master".to_owned()),
        ]),
        Driver::Mongodb
        | Driver::Memcached
        | Driver::Redis
        | Driver::Valkey
        | Driver::Dragonfly
        | Driver::Garage
        | Driver::Rustfs
        | Driver::Localstack
        | Driver::Opensearch
        | Driver::Elasticsearch
        | Driver::Meilisearch
        | Driver::Typesense
        | Driver::Frankenphp
        | Driver::Reverb
        | Driver::Horizon
        | Driver::Scheduler
        | Driver::Dusk
        | Driver::Gotenberg
        | Driver::Mailhog
        | Driver::Rabbitmq
        | Driver::Soketi => {
            anyhow::bail!(
                "`berth restore` loads SQL into PostgreSQL, MySQL or MariaDB services only; pass \
                 `--service` with one of those"
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::{Driver, ServiceConfig};

    use super::{
        build_client_exec_args, build_exec_args, client_env, interactive_client_args,
        piped_client_args, piped_client_env,
    };

    fn service(
        name: &str,
        driver: Driver,
        username: Option<&str>,
        password: Option<&str>,
        database: Option<&str>,
    ) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind: crate::config::Kind::App,
            driver,
            image: "app".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 9000,
            database: database.map(ToOwned::to_owned),
            username: username.map(ToOwned::to_owned),
            password: password.map(ToOwned::to_owned),
            bucket: None,
            access_key: None,
            secret_key: None,
            api_key: None,
            region: None,
            scheme: None,
            domain: None,
            domains: None,
            resolved_domain: None,
            routes: None,
            container_port: None,
            smtp_port: None,
            volumes: None,
            env: None,
            command: None,
            depends_on: None,
            seed_file: None,
            hook: Vec::new(),
            health_path: None,
            health_statuses: None,
            restart: None,
            localhost_tls: false,
            octane: false,
            octane_workers: None,
            octane_max_requests: None,
            php_extensions: None,
            trust_container_ca: false,
            env_mapping: None,
            javascript: None,
            container_name: None,
            resolved_container_name: None,
            runtime: Default::default(),
        }
    }

    #[test]
    fn build_exec_args_includes_tty_option() {
        let args = build_exec_args("container", &["echo".to_owned(), "hi".to_owned()], true);
        assert_eq!(args, vec!["exec", "-it", "container", "echo", "hi"]);
    }

    #[test]
    fn interactive_client_args_uses_defaults() {
        let postgres = service("db", Driver::Postgres, None, None, None);
        assert_eq!(
            interactive_client_args(&postgres),
            vec![
                "psql".to_owned(),
                "-U".to_owned(),
                "postgres".to_owned(),
                "app".to_owned()
            ]
        );

        let mysql = service(
            "db",
            Driver::Mysql,
            Some("root"),
            Some("secret"),
            Some("db"),
        );
        assert_eq!(
            interactive_client_args(&mysql),
            vec![
                "mysql".to_owned(),
                "--protocol=tcp".to_owned(),
                "-h".to_owned(),
                "127.0.0.1".to_owned(),
                "-u".to_owned(),
                "root".to_owned(),
                "db".to_owned()
            ]
        );

        let redis = service("cache", Driver::Redis, None, None, None);
        assert_eq!(
            interactive_client_args(&redis),
            vec![
                "redis-cli".to_owned(),
                "-h".to_owned(),
                "127.0.0.1".to_owned(),
                "-p".to_owned(),
                "6379".to_owned()
            ]
        );

        let web = service("app", Driver::Frankenphp, None, None, None);
        assert_eq!(interactive_client_args(&web), vec!["sh".to_owned()]);
    }

    #[test]
    fn mysql_clients_follow_the_image_and_keep_the_password_out_of_arguments() {
        let mut mariadb = service(
            "db",
            Driver::Mysql,
            Some("u"),
            Some("pw-12345"),
            Some("app"),
        );
        mariadb.image = "mariadb:11".to_owned();
        let command = interactive_client_args(&mariadb);
        assert_eq!(command[0], "mariadb", "MariaDB 11 images have no `mysql`");
        assert!(!command.iter().any(|arg| arg.contains("pw-12345")));
        let args = build_client_exec_args("c", &client_env(&mariadb), &command, false);
        assert_eq!(args[..5], ["exec", "-i", "-e", "MYSQL_PWD=pw-12345", "c"]);
    }

    #[test]
    fn a_restores_postgres_client_asks_for_english_messages_on_both_sides() {
        // The restore judges psql's stderr by its English wording: the
        // server's (`lc_messages`) and psql's own (`LC_ALL`).
        let postgres = service("db", Driver::Postgres, None, None, Some("app"));
        let env = piped_client_env(&postgres);
        assert!(
            env.contains(
                &"PGOPTIONS=-c lc_messages=C -c standard_conforming_strings=on".to_owned()
            ),
            "{env:?}"
        );
        assert!(env.contains(&"LC_ALL=C".to_owned()), "{env:?}");
        let mysql = service("db", Driver::Mysql, None, None, Some("app"));
        assert!(
            !piped_client_env(&mysql)
                .iter()
                .any(|var| var.starts_with("PGOPTIONS") || var.starts_with("LC_ALL")),
            "MySQL's restore reads its exit status"
        );
    }

    #[test]
    fn a_reset_accepts_only_its_own_missing_database() {
        let named = service("db", Driver::Postgres, None, None, Some("shop"));
        let unnamed = service("db", Driver::Postgres, None, None, None);
        let missing = |service: &ServiceConfig, stderr: &str| {
            super::super::client_reachable::reports_missing_database(service, stderr)
        };
        assert!(missing(&named, "FATAL:  database \"shop\" does not exist"));
        assert!(missing(
            &named,
            "ERROR 1049 (42000): Unknown database 'shop'"
        ));
        // The client's fallback name, which the reset recreates too.
        assert!(missing(&unnamed, "FATAL:  database \"app\" does not exist"));
        assert!(!missing(
            &named,
            "FATAL:  database \"other\" does not exist"
        ));
        assert!(!missing(&named, "FATAL:  password authentication failed"));
    }

    #[test]
    fn piped_client_args_supports_only_sql_backends() {
        let redis = service("cache", Driver::Redis, None, None, None);
        assert!(piped_client_args(&redis).is_err());

        let postgres = service("db", Driver::Postgres, None, None, Some("database"));
        let args = piped_client_args(&postgres).expect("postgres args");
        // berth's markers run around the dump, and the connection comes last.
        assert_eq!(args[..3], ["psql", "-X", "-b"]);
        let dump = args.iter().position(|arg| arg == "-").expect("-f -");
        assert!(
            args[..dump]
                .iter()
                .any(|arg| arg.contains("berth-backend-start"))
        );
        assert!(
            args[dump..]
                .iter()
                .any(|arg| arg.contains("berth-backend-end"))
        );
        assert_eq!(args[args.len() - 3..], ["-U", "postgres", "database"]);

        let sqlserver = service(
            "mssql",
            Driver::Sqlserver,
            Some("sa"),
            Some("password"),
            Some("master"),
        );
        assert_eq!(
            piped_client_args(&sqlserver).expect("sqlserver args"),
            vec![
                "sqlcmd".to_owned(),
                "-S".to_owned(),
                "localhost".to_owned(),
                "-U".to_owned(),
                "sa".to_owned(),
                "-P".to_owned(),
                "password".to_owned(),
                "-d".to_owned(),
                "master".to_owned()
            ]
        );
    }
    #[test]
    fn a_piped_mysql_client_prints_rows_without_column_names() {
        // berth reads its own rows after the dump from the client's output.
        let mysql = service("db", Driver::Mysql, None, None, Some("app"));
        assert!(
            piped_client_args(&mysql)
                .expect("mysql args")
                .contains(&"--skip-column-names".to_owned())
        );
    }
}
