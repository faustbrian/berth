//! cli handlers dump cmd module.
//!
//! Contains cli handlers dump cmd logic used by Berth command workflows.

use anyhow::Result;
use std::path::PathBuf;

use crate::{cli, config, database};

pub(crate) struct HandleDumpOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) file: Option<&'a PathBuf>,
    pub(crate) stdout: bool,
    pub(crate) gzip: bool,
}

pub(crate) fn handle_dump(config: &config::Config, options: HandleDumpOptions<'_>) -> Result<()> {
    let svc = config::resolve_service(config, options.service)?;
    cli::support::ensure_sql_service(svc, "dump")?;

    if options.stdout {
        database::dump_stdout(svc, options.gzip)?;
    } else if let Some(path) = options.file {
        database::dump(svc, path, options.gzip)?;
    } else {
        anyhow::bail!("specify --file or --stdout");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use std::env;
    use std::fs;
    use std::io::Write;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    use crate::config;
    use crate::docker;

    fn service(
        name: &str,
        kind: config::Kind,
        driver: config::Driver,
        container_name: &str,
    ) -> config::ServiceConfig {
        config::ServiceConfig {
            name: name.to_owned(),
            kind,
            driver,
            image: "service:latest".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 3306,
            database: Some("app".to_owned()),
            username: None,
            password: None,
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
            container_name: Some(container_name.to_owned()),
            resolved_container_name: None,
            runtime: Default::default(),
        }
    }

    fn config_with_services(services: Vec<config::ServiceConfig>) -> config::Config {
        config::Config {
            schema_version: 1,
            project_type: config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: services,
            swarm: Vec::new(),
        }
    }

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

    #[test]
    fn handle_dump_prints_to_stdout_when_requested() -> Result<()> {
        let config = config_with_services(vec![service(
            "db",
            config::Kind::Database,
            config::Driver::Mysql,
            "db-container",
        )]);

        let file = env::temp_dir().join(format!(
            "berth-dump-cmd-stdout-{}.sql",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));

        docker::with_dry_run_lock(|| {
            super::handle_dump(
                &config,
                super::HandleDumpOptions {
                    service: Some("db"),
                    file: Some(&file),
                    stdout: true,
                    gzip: false,
                },
            )
        })?;
        Ok(())
    }

    #[test]
    fn handle_dump_writes_to_file_when_path_is_provided() -> Result<()> {
        let config = config_with_services(vec![service(
            "db",
            config::Kind::Database,
            config::Driver::Postgres,
            "db-container",
        )]);
        let file = env::temp_dir().join(format!(
            "berth-dump-cmd-file-{}.sql",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));

        let command = fake_docker_binary(r#"printf '%s ' "$@"; printf '\n'; exit 0"#);

        docker::with_docker_command(&command, || {
            docker::with_dry_run_lock(|| {
                super::handle_dump(
                    &config,
                    super::HandleDumpOptions {
                        service: Some("db"),
                        file: Some(&file),
                        stdout: false,
                        gzip: true,
                    },
                )
            })
        })?;
        Ok(())
    }

    #[test]
    fn handle_dump_requires_file_or_stdout() -> Result<()> {
        let config = config_with_services(vec![service(
            "db",
            config::Kind::Database,
            config::Driver::Postgres,
            "db-container",
        )]);

        docker::with_dry_run_lock(|| {
            let result = super::handle_dump(
                &config,
                super::HandleDumpOptions {
                    service: Some("db"),
                    file: None,
                    stdout: false,
                    gzip: false,
                },
            );
            assert!(result.is_err());
            assert_eq!(
                result.unwrap_err().to_string(),
                "specify --file or --stdout"
            );
        });
        Ok(())
    }

    #[test]
    fn handle_dump_rejects_non_database_service() -> Result<()> {
        let config = config_with_services(vec![service(
            "app",
            config::Kind::App,
            config::Driver::Frankenphp,
            "app-container",
        )]);
        let path = env::temp_dir().join(format!(
            "berth-dump-cmd-invalid-{}.sql",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));

        let result = super::handle_dump(
            &config,
            super::HandleDumpOptions {
                service: Some("app"),
                file: Some(&path),
                stdout: false,
                gzip: false,
            },
        );

        assert!(
            result.is_err_and(|error| error
                .to_string()
                .contains("supports SQL database services only")),
            "expected sql service validation failure"
        );
        Ok(())
    }
}
