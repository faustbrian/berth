//! Shared computed fields for open-summary workflows.

use anyhow::Result;

use crate::{config, serve};

pub(crate) struct OpenSummaryData {
    pub(crate) app_url: String,
    pub(crate) health_url: String,
    pub(crate) health_status: Option<u16>,
    pub(crate) health_ok: bool,
}

pub(crate) fn open_summary_data(
    serve_target: &config::ServiceConfig,
    health_path: Option<&str>,
) -> Result<OpenSummaryData> {
    let app_url = serve::public_url(serve_target)?;
    let health_url = super::build_health_url(serve_target, &app_url, health_path);
    let health_status = super::probe_http_status(&health_url);
    let health_ok = health_status
        .map(|status| super::health_status_accepted(serve_target, status))
        .unwrap_or(false);

    Ok(OpenSummaryData {
        app_url,
        health_url,
        health_status,
        health_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::support::probe_http_status::with_curl_command;
    use crate::config::{Driver, Kind, ServiceConfig};
    use std::fs;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static SUMMARY_FIXTURE_COUNTER: AtomicU64 = AtomicU64::new(0);

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
            scheme: None,
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

    fn with_fake_curl<F, T>(output: &str, status_ok: bool, test: F) -> T
    where
        F: FnOnce() -> T,
    {
        let id = SUMMARY_FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let bin_dir = crate::test_support::temp_root().join(format!(
            "berth-summary-{}-{}-{}",
            std::process::id(),
            id,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&bin_dir).expect("temp dir");

        let script = bin_dir.join("curl");
        let mut file = fs::File::create(&script).expect("create script");
        if status_ok {
            writeln!(file, "#!/bin/sh\nprintf '{}'; exit 0", output).expect("write script");
        } else {
            writeln!(file, "#!/bin/sh\n{}", output).expect("write script");
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
        fs::remove_dir_all(&bin_dir).ok();
        result
    }

    #[test]
    fn open_summary_data_builds_health_and_status_fields() {
        with_fake_curl("200", true, || {
            let summary = open_summary_data(&make_target(), Some("/health")).expect("summary");
            assert_eq!(summary.app_url, "https://localhost:8080");
            assert_eq!(summary.health_url, "https://localhost:8080/health");
            assert_eq!(summary.health_status, Some(200));
            assert!(summary.health_ok);
        });
    }

    #[test]
    fn open_summary_data_handles_unreachable_health() {
        with_fake_curl("exit 1", false, || {
            let summary = open_summary_data(&make_target(), Some("/health")).expect("summary");
            assert_eq!(summary.health_status, None);
            assert!(!summary.health_ok);
        });
    }
}
