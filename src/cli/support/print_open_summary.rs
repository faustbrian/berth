//! cli support print open summary module.
//!
//! Contains cli support print open summary logic used by Berth command workflows.

use anyhow::Result;

use crate::config;
use crate::output::{self, LogLevel, Persistence};

use super::open_in_browser::open_in_browser;

pub(crate) fn print_open_summary(
    serve_target: &config::ServiceConfig,
    health_path: Option<&str>,
    no_browser: bool,
) -> Result<()> {
    let summary = super::open_summary_data(serve_target, health_path)?;
    output::event(
        &serve_target.name,
        LogLevel::Info,
        &format!("App URL: {}", summary.app_url),
        Persistence::Persistent,
    );

    if !no_browser {
        open_in_browser(&summary.app_url);
    }

    emit_health_status(serve_target, &summary.health_url, summary.health_status);

    Ok(())
}

fn emit_health_status(serve_target: &config::ServiceConfig, health_url: &str, status: Option<u16>) {
    let (level, message) = match status {
        Some(code) if super::health_status_accepted(serve_target, code) => {
            (LogLevel::Success, format!("Health: {health_url} ({code})"))
        }
        Some(code) => (LogLevel::Warn, format!("Health: {health_url} ({code})")),
        None => (
            LogLevel::Warn,
            format!("Health: {health_url} (unreachable)"),
        ),
    };

    output::event(&serve_target.name, level, &message, Persistence::Persistent);
}

#[cfg(test)]
mod tests {
    use super::print_open_summary;
    use crate::cli;
    use crate::config::{Driver, Kind, ServiceConfig};
    use std::fs;
    use std::io::Write;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_target() -> ServiceConfig {
        ServiceConfig {
            name: "app".to_owned(),
            kind: Kind::App,
            driver: Driver::Frankenphp,
            image: "dunglas/frankenphp:php8.5".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 8080,
            database: None,
            username: None,
            password: None,
            bucket: None,
            access_key: None,
            secret_key: None,
            api_key: None,
            region: None,
            scheme: Some("https".to_owned()),
            domain: Some("app.berth".to_owned()),
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
            container_name: Some("app".to_owned()),
            resolved_container_name: Some("app".to_owned()),
            runtime: Default::default(),
        }
    }

    fn with_fake_open_command<F, T>(test: F) -> T
    where
        F: FnOnce(&Path, &Path) -> T,
    {
        let marker_dir = crate::test_support::temp_root().join(format!(
            "berth-print-open-summary-{}",
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
            "#!/bin/sh\nprintf '%s\\n' \"$1\" > \"{}\"",
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

        let result = test(&marker, &command);
        fs::remove_dir_all(marker_dir).ok();
        result
    }

    #[test]
    fn print_open_summary_does_not_open_browser_when_disabled() {
        with_fake_open_command(|marker, open| {
            cli::support::with_curl_command("printf '200'; exit 0", || {
                cli::support::with_open_command(open.to_str().expect("open path"), || {
                    print_open_summary(&make_target(), None, true).expect("no-browser summary");
                });
            });
            assert!(!marker.exists());
        });
    }

    #[test]
    fn print_open_summary_opens_browser_when_enabled() {
        with_fake_open_command(|marker, open| {
            cli::support::with_curl_command("printf '200'; exit 0", || {
                cli::support::with_open_command(open.to_str().expect("open path"), || {
                    print_open_summary(&make_target(), None, false).expect("summary with browser");
                });
            });
            let invoked = fs::read_to_string(marker).expect("open marker");
            assert_eq!(invoked, "https://localhost:8080\n");
        });
    }
}
