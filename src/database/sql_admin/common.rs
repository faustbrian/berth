//! database sql admin common module.
//!
//! Contains database sql admin common logic used by Berth command workflows.

use anyhow::Result;
use std::process::Output;

use crate::config::{Driver, ServiceConfig};
use crate::output::{self, LogLevel, Persistence};

pub(super) struct SqlContext {
    pub(super) driver: Driver,
    pub(super) container_name: String,
    pub(super) db_name: String,
    pub(super) username: String,
    pub(super) password: String,
}

pub(super) fn sql_context(service: &ServiceConfig) -> Result<SqlContext> {
    let driver = sql_driver(service)?;

    Ok(SqlContext {
        driver,
        container_name: service.container_name()?,
        db_name: service.database.as_deref().unwrap_or("app").to_owned(),
        username: service.username.as_deref().unwrap_or("root").to_owned(),
        password: service.password.as_deref().unwrap_or("secret").to_owned(),
    })
}

pub(super) fn sql_driver(service: &ServiceConfig) -> Result<Driver> {
    match service.driver {
        Driver::Postgres | Driver::Mysql => Ok(service.driver),
        Driver::Mongodb
        | Driver::Memcached
        | Driver::Sqlserver
        | Driver::Redis
        | Driver::Valkey
        | Driver::Dragonfly
        | Driver::Minio
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
        | Driver::Soketi => anyhow::bail!("service '{}' is not SQL", service.name),
    }
}

pub(super) fn run_docker_command_owned(args: &[String], context: &str) -> Result<Output> {
    crate::docker::run_docker_output_owned(args, context)
}

pub(super) fn run_postgres_exec(
    ctx: &SqlContext,
    program: &str,
    args: &[String],
    context: &str,
) -> Result<Output> {
    let mut command = vec![
        "exec".to_owned(),
        ctx.container_name.clone(),
        program.to_owned(),
        "-U".to_owned(),
        ctx.username.clone(),
    ];
    command.extend(args.iter().cloned());
    run_docker_command_owned(&command, context)
}

pub(super) fn run_mysql_exec(
    ctx: &SqlContext,
    program: &str,
    args: &[String],
    context: &str,
) -> Result<Output> {
    let mut command = vec![
        "exec".to_owned(),
        ctx.container_name.clone(),
        program.to_owned(),
        "-u".to_owned(),
        ctx.username.clone(),
        mysql_password_flag(&ctx.password),
    ];
    command.extend(args.iter().cloned());
    run_docker_command_owned(&command, context)
}

pub(super) fn mysql_password_flag(password: &str) -> String {
    format!("-p{password}")
}

pub(super) fn ensure_command_success(output: &Output, failure_prefix: &str) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.is_empty() {
        anyhow::bail!("{failure_prefix} (exit code: {:?})", output.status.code());
    }
    anyhow::bail!("{failure_prefix}: {stderr}");
}

pub(super) fn emit_sql_admin_dry_run(service: &ServiceConfig, message: &str) {
    output::event(
        &service.name,
        LogLevel::Info,
        message,
        Persistence::Transient,
    );
}

#[cfg(test)]
mod tests {
    use crate::config::{Driver, Kind, ServiceConfig};
    use crate::docker;
    use std::env;
    use std::fs;
    use std::io::Write;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    use super::*;

    fn fake_docker_binary(script: &str) -> String {
        let bin_dir = env::temp_dir().join(format!(
            "berth-fake-docker-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        fs::create_dir_all(&bin_dir).expect("create fake docker dir");
        let binary = bin_dir.join("docker");
        let mut file = fs::File::create(&binary).expect("create fake docker");
        writeln!(file, "#!/bin/sh\n{}", script).expect("write fake script");
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&binary).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&binary, perms).expect("set mode");
        }

        binary.to_string_lossy().to_string()
    }

    fn service(driver: Driver) -> ServiceConfig {
        ServiceConfig {
            name: "app".to_owned(),
            kind: Kind::App,
            driver,
            image: "php:8.4".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 9000,
            database: Some("app".to_owned()),
            username: Some("root".to_owned()),
            password: Some("secret".to_owned()),
            bucket: None,
            access_key: None,
            secret_key: None,
            api_key: None,
            region: None,
            scheme: None,
            domain: None,
            domains: None,
            resolved_domain: None,
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
            container_name: Some("app".to_owned()),
            resolved_container_name: Some("app".to_owned()),
            runtime: Default::default(),
        }
    }

    #[test]
    fn sql_driver_accepts_postgres_and_mysql() {
        assert!(matches!(
            sql_driver(&service(Driver::Postgres)),
            Ok(Driver::Postgres)
        ));
        assert!(matches!(
            sql_driver(&service(Driver::Mysql)),
            Ok(Driver::Mysql)
        ));
    }

    #[test]
    fn sql_driver_rejects_unsupported_driver() {
        let err = sql_driver(&service(Driver::Redis)).expect_err("expected SQL driver error");
        assert!(err.to_string().contains("is not SQL"));
    }

    #[test]
    fn sql_context_uses_defaults_for_optional_fields() {
        let mut target = service(Driver::Postgres);
        target.database = None;
        target.username = None;
        target.password = None;

        let ctx = sql_context(&target).expect("sql context");
        assert_eq!(ctx.db_name, "app");
        assert_eq!(ctx.username, "root");
        assert_eq!(ctx.password, "secret");
    }

    #[test]
    fn sql_command_helpers_include_expected_args() {
        let ctx = sql_context(&service(Driver::Postgres)).expect("sql context");
        let binary = fake_docker_binary("printf '%s ' \"$@\"; printf '\\n'; exit 0");
        let output = docker::with_docker_command(&binary, || {
            run_postgres_exec(
                &ctx,
                "psql",
                &["-d".to_owned(), "postgres".to_owned()],
                "run",
            )
            .expect("run postgres command")
        });

        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "exec app psql -U root -d postgres"
        );

        let ctx = sql_context(&service(Driver::Mysql)).expect("sql context");
        let output = docker::with_docker_command(&binary, || {
            run_mysql_exec(
                &ctx,
                "mysql",
                &["-e".to_owned(), "select 1".to_owned()],
                "run",
            )
            .expect("run mysql command")
        });

        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "exec app mysql -u root -psecret -e select 1"
        );
    }

    #[test]
    fn mysql_password_flag_formats_argument() {
        assert_eq!(mysql_password_flag("abc"), "-pabc");
    }

    #[test]
    fn ensure_command_success_reports_errors_when_output_fails() {
        let failed = std::process::Command::new("sh")
            .arg("-c")
            .arg("exit 1")
            .output()
            .expect("failed command");
        let mut failed_output = failed;
        failed_output.stderr = b"boom".to_vec();

        let err =
            ensure_command_success(&failed_output, "prefix").expect_err("expected command failure");
        assert!(err.to_string().contains("prefix"));
        assert!(err.to_string().contains("boom"));

        let failed_no_stderr = std::process::Command::new("sh")
            .arg("-c")
            .arg("exit 2")
            .output()
            .expect("failed command");
        let failed_no_stderr = Output {
            status: failed_no_stderr.status,
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        let err =
            ensure_command_success(&failed_no_stderr, "prefix").expect_err("expected code failure");
        assert!(err.to_string().contains("exit code: Some(2)"));
    }
}
