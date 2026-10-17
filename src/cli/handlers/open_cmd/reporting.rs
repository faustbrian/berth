//! `open` command reporting helpers.

use anyhow::Result;

use crate::cli::handlers::serialize;
use crate::output::{self, LogLevel, Persistence};
use crate::{cli, config, docker};

use super::database_runtime_url;

pub(super) fn render_open_json(
    config: &config::Config,
    targets: &[&config::ServiceConfig],
    health_path: Option<&str>,
) -> Result<()> {
    let app_summaries: Vec<serde_json::Value> = targets
        .iter()
        .map(|target| cli::support::build_open_summary_json(target, health_path))
        .collect::<Result<Vec<_>>>()?;
    let databases: Vec<serde_json::Value> = database_services(config)
        .into_iter()
        .map(|svc| {
            serde_json::json!({
                "name": svc.name,
                "status": service_container_status(svc),
                "url": database_runtime_url(svc),
            })
        })
        .collect();
    serialize::print_json_pretty(&serde_json::json!({
        "apps": app_summaries,
        "databases": databases
    }))?;

    Ok(())
}

pub(super) fn render_open_text(
    config: &config::Config,
    targets: &[&config::ServiceConfig],
    health_path: Option<&str>,
    no_browser: bool,
    database: bool,
) -> Result<()> {
    if !database {
        for target in targets {
            cli::support::print_open_summary(target, health_path, no_browser)?;
        }
    }

    let databases = database_services(config);
    if databases.is_empty() {
        emit_open_info("DB status: no database services configured");
        return Ok(());
    }

    emit_open_info("DB status:");
    for svc in databases {
        let url = database_runtime_url(svc);
        emit_open_info(&format!(
            "{}: {} ({})",
            svc.name,
            service_container_status(svc),
            url
        ));
        if database {
            cli::support::open_in_browser(&url);
        }
    }

    Ok(())
}

fn service_container_status(service: &config::ServiceConfig) -> String {
    service
        .container_name()
        .ok()
        .and_then(|name| docker::inspect_status(&name))
        .unwrap_or_else(|| "not created".to_owned())
}

fn database_services(config: &config::Config) -> Vec<&config::ServiceConfig> {
    config
        .service
        .iter()
        .filter(|svc| svc.kind == config::Kind::Database)
        .collect()
}

fn emit_open_info(message: &str) {
    output::event("open", LogLevel::Info, message, Persistence::Persistent);
}

#[cfg(test)]
mod tests {
    use super::{database_services, render_open_json, render_open_text, service_container_status};
    use crate::cli::support::with_curl_command;
    use crate::config::{Config, Driver, Kind, ServiceConfig};
    use crate::docker;
    use std::env;
    use std::fs;
    use std::io::Write;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn service(
        name: &str,
        kind: Kind,
        driver: Driver,
        container_name: Option<&str>,
        localhost_tls: bool,
    ) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind,
            driver,
            image: "php".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 9000,
            database: None,
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
            localhost_tls,
            octane: false,
            octane_workers: None,
            octane_max_requests: None,
            php_extensions: None,
            trust_container_ca: false,
            env_mapping: None,
            javascript: None,
            container_name: container_name.map(ToOwned::to_owned),
            resolved_container_name: None,
            runtime: Default::default(),
        }
    }

    fn config() -> Config {
        Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![
                service("db", Kind::Database, Driver::Postgres, None, false),
                service("app", Kind::App, Driver::Frankenphp, None, true),
            ],
            swarm: Vec::new(),
        }
    }

    fn with_fake_curl<F, T>(output: &str, status_ok: bool, test: F) -> T
    where
        F: FnOnce() -> T,
    {
        let dir = env::temp_dir().join(format!(
            "berth-open-reporting-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        let script = dir.join("curl");
        let mut file = fs::File::create(&script).expect("create fake curl");
        if status_ok {
            writeln!(file, "#!/bin/sh\nprintf '{}'; exit 0", output).expect("write fake script");
        } else {
            writeln!(file, "#!/bin/sh\n{}", output).expect("write fake script");
        }
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script, perms).expect("chmod");
        }

        let command = script.to_string_lossy().to_string();
        let result = with_curl_command(&command, test);
        fs::remove_dir_all(&dir).ok();
        result
    }

    fn with_fake_open_command<F, T>(test: F) -> T
    where
        F: FnOnce(&Path) -> T,
    {
        let marker_dir = env::temp_dir().join(format!(
            "berth-open-reporting-open-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&marker_dir).expect("create marker dir");
        let marker = marker_dir.join("invoked");

        let command = marker_dir.join("open");
        let mut file = fs::File::create(&command).expect("create fake open command");
        writeln!(
            file,
            "#!/bin/sh\nprintf '%s\\n' \"$1\" >> \"{}\"",
            marker.display()
        )
        .expect("write fake open");
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&command).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&command, perms).expect("chmod");
        }

        let result =
            crate::cli::support::with_open_command(command.to_str().expect("open path"), || {
                test(&marker_dir)
            });
        fs::remove_dir_all(&marker_dir).ok();
        result
    }

    fn with_fake_docker_status_none<F, T>(test: F) -> T
    where
        F: FnOnce() -> T,
    {
        docker::with_docker_command("exit 1", test)
    }

    #[test]
    fn render_open_text_reports_empty_database_state() -> anyhow::Result<()> {
        let cfg = Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: Vec::new(),
            swarm: Vec::new(),
        };
        assert_eq!(
            service_container_status(&service(
                "db",
                Kind::Database,
                Driver::Postgres,
                None,
                false
            )),
            "not created"
        );
        let targets = Vec::new();
        render_open_text(&cfg, &targets, None, false, false)?;
        Ok(())
    }

    #[test]
    fn render_open_text_covers_database_service_list() -> anyhow::Result<()> {
        let cfg = config();
        let targets = Vec::new();
        assert_eq!(database_services(&cfg)[0].name, "db");
        render_open_text(&cfg, &targets, Some("/health"), false, false)?;
        Ok(())
    }

    #[test]
    fn render_open_text_opens_browser_for_app_target() -> anyhow::Result<()> {
        with_fake_curl("200", true, || {
            let cfg = config();
            let targets: Vec<&ServiceConfig> = cfg
                .service
                .iter()
                .filter(|svc| svc.kind == Kind::App)
                .collect();
            let opened = with_fake_open_command(|marker_dir| {
                render_open_text(&cfg, &targets, None, false, false)
                    .expect("render open summary with browser");
                fs::read_to_string(marker_dir.join("invoked")).expect("fake open marker")
            });
            assert_eq!(opened, "https://localhost:9000\n");
            Ok(())
        })
    }

    #[test]
    fn render_open_text_reports_database_status_when_container_is_missing() -> anyhow::Result<()> {
        with_fake_curl("200", true, || {
            with_fake_docker_status_none(|| {
                let cfg = config();
                let targets = Vec::new();
                render_open_text(&cfg, &targets, None, true, false)?;
                assert_eq!(service_container_status(&cfg.service[0]), "not created");
                Ok(())
            })
        })
    }

    #[test]
    fn render_open_text_opens_database_connection_when_enabled() -> anyhow::Result<()> {
        with_fake_curl("200", true, || {
            with_fake_docker_status_none(|| {
                let cfg = config();
                let targets = Vec::new();
                let opened = with_fake_open_command(|marker_dir| {
                    render_open_text(&cfg, &targets, None, true, true)
                        .expect("render open summary with database open");
                    fs::read_to_string(marker_dir.join("invoked")).expect("fake open marker")
                });
                assert_eq!(opened, "postgresql://root@127.0.0.1:9000/app\n");
                Ok(())
            })
        })
    }

    #[test]
    fn render_open_text_database_mode_skips_app_browser_open() -> anyhow::Result<()> {
        with_fake_curl("200", true, || {
            with_fake_docker_status_none(|| {
                let cfg = config();
                let targets: Vec<&ServiceConfig> = cfg
                    .service
                    .iter()
                    .filter(|svc| svc.kind == Kind::App)
                    .collect();
                let opened = with_fake_open_command(|marker_dir| {
                    render_open_text(&cfg, &targets, None, false, true)
                        .expect("render open summary in database mode");
                    fs::read_to_string(marker_dir.join("invoked")).expect("fake open marker")
                });
                assert_eq!(opened, "postgresql://root@127.0.0.1:9000/app\n");
                Ok(())
            })
        })
    }

    #[test]
    fn render_open_json_serializes_open_targets_and_databases() -> anyhow::Result<()> {
        with_fake_curl("200", true, || {
            let cfg = config();
            let targets: Vec<&ServiceConfig> = cfg
                .service
                .iter()
                .filter(|svc| svc.kind == Kind::App)
                .collect();
            render_open_json(&cfg, &targets, Some("/health"))?;
            Ok(())
        })
    }
}
