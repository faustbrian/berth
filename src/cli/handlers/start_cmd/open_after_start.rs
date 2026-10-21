//! start command open-after-start handling.

use anyhow::Result;

use crate::cli::handlers::log;
use crate::config;

use super::super::{HandleOpenOptions, handle_open};

pub(super) fn maybe_open_after_start(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    open_after_start: bool,
    health_path: Option<&str>,
    quiet: bool,
) -> Result<()> {
    if !open_after_start {
        return Ok(());
    }

    let only_non_app_kind = kind.is_some_and(|value| value != config::Kind::App);
    if only_non_app_kind {
        log::info_if_not_quiet(
            quiet,
            "start",
            "Skipping app URL summary because selected kind has no app services",
        );
        return Ok(());
    }

    if let Some(service_name) = service {
        let selected = config::find_service(config, service_name)?;
        if selected.kind != config::Kind::App {
            log::info_if_not_quiet(
                quiet,
                "start",
                &format!(
                    "Skipping app URL summary because '{}' is not an app service",
                    selected.name
                ),
            );
            return Ok(());
        }
        return handle_open(
            config,
            HandleOpenOptions {
                service: Some(service_name),
                kind: Some(config::Kind::App),
                profile: None,
                all: false,
                health_path,
                no_browser: false,
                non_interactive: false,
                database: false,
                json: false,
            },
        );
    }

    handle_open(
        config,
        HandleOpenOptions {
            service: None,
            kind: Some(config::Kind::App),
            profile: None,
            all: true,
            health_path,
            no_browser: false,
            non_interactive: false,
            database: false,
            json: false,
        },
    )
}

#[cfg(test)]
mod tests {
    use crate::cli::support;
    use crate::config::{Config, Driver, Kind, ServiceConfig};
    use std::fs;
    use std::io::Write;
    use std::path::Path;

    use super::maybe_open_after_start;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn service(name: &str, kind: Kind, domain: Option<&str>) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind,
            driver: Driver::Frankenphp,
            image: "dunglas/frankenphp:php8.5".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 33065,
            database: None,
            username: None,
            password: None,
            bucket: None,
            access_key: None,
            secret_key: None,
            api_key: None,
            region: None,
            scheme: Some("https".to_owned()),
            domain: domain.map(ToOwned::to_owned),
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
            localhost_tls: true,
            octane: false,
            octane_workers: None,
            octane_max_requests: None,
            php_extensions: None,
            trust_container_ca: false,
            env_mapping: None,
            javascript: None,
            container_name: Some(name.to_owned()),
            resolved_container_name: Some(name.to_owned()),
            runtime: Default::default(),
        }
    }

    fn with_fake_command<F, T>(name: &str, script_body: &str, test: F) -> T
    where
        F: FnOnce(&Path, &str) -> T,
    {
        let bin_dir = crate::test_support::temp_root().join(format!(
            "berth-open-after-start-{}",
            crate::test_support::unique_stamp()
        ));
        fs::create_dir_all(&bin_dir).expect("create temp dir");
        let command = bin_dir.join(name);
        let mut file = fs::File::create(&command).expect("create fake command");
        writeln!(file, "#!/bin/sh\n{}", script_body).expect("write script");
        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&command).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&command, perms).expect("chmod");
        }

        let command_path = command.to_string_lossy().to_string();
        let result = test(&bin_dir, &command_path);
        fs::remove_dir_all(Path::new(&bin_dir)).ok();

        result
    }

    fn config() -> Config {
        Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![
                service("app", Kind::App, Some("app.berth")),
                service("cache", Kind::Database, Some("redis.berth")),
            ],
            swarm: Vec::new(),
        }
    }

    #[test]
    fn maybe_open_after_start_noop_when_disabled() {
        maybe_open_after_start(&config(), Some("app"), None, false, None, false)
            .expect("expected no-op when disabled");
    }

    #[test]
    fn maybe_open_after_start_skips_non_app_kind() {
        maybe_open_after_start(&config(), None, Some(Kind::Database), true, None, false)
            .expect("expected skip for non-app kind");
    }

    #[test]
    fn maybe_open_after_start_skips_non_app_service() {
        maybe_open_after_start(&config(), Some("cache"), Some(Kind::App), true, None, false)
            .expect("expected skip for non-app selected service");
    }

    #[test]
    fn maybe_open_after_start_opening_selected_app_service() {
        let open_command = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };

        with_fake_command(open_command, ":", |_, open_path| {
            support::with_open_command(open_path, || {
                with_fake_command("curl", "printf '200'", |_, curl_path| {
                    let result = support::with_curl_command(curl_path, || {
                        let cfg = config();
                        maybe_open_after_start(
                            &cfg,
                            Some("app"),
                            Some(Kind::App),
                            true,
                            None,
                            false,
                        )
                    });
                    assert!(result.is_ok());
                })
            })
        });
    }
}
