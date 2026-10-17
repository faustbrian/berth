//! cli support build open summary json module.
//!
//! Contains cli support build open summary json logic used by Berth command workflows.

use anyhow::Result;

use crate::config;

/// Builds open summary json for command execution.
pub(crate) fn build_open_summary_json(
    serve_target: &config::ServiceConfig,
    health_path: Option<&str>,
) -> Result<serde_json::Value> {
    let summary = super::open_summary_data(serve_target, health_path)?;
    Ok(serde_json::json!({
        "name": serve_target.name,
        "app_url": summary.app_url,
        "health_url": summary.health_url,
        "health_status": summary.health_status,
        "health_ok": summary.health_ok
    }))
}

#[cfg(test)]
mod tests {
    use super::build_open_summary_json;
    use crate::cli::support::probe_http_status::with_curl_command;
    use crate::config::{Driver, Kind, ServiceConfig};
    use serde_json::Value;
    use std::env;
    use std::fs;
    use std::io::Write;
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
            scheme: None,
            domain: Some("app.berth".to_owned()),
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

    fn with_fake_curl<F, T>(script: &str, test: F) -> T
    where
        F: FnOnce() -> T,
    {
        let bin_dir = env::temp_dir().join(format!(
            "berth-summary-json-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&bin_dir).expect("temp dir");
        let curl = bin_dir.join("curl");
        let mut file = fs::File::create(&curl).expect("create script");
        writeln!(file, "#!/bin/sh\n{}", script).expect("write script");
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&curl).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&curl, perms).expect("chmod");
        }
        let command = curl.to_string_lossy().to_string();
        let result = with_curl_command(&command, test);
        fs::remove_dir_all(bin_dir).ok();
        result
    }

    #[test]
    fn build_open_summary_json_returns_summary_payload() {
        with_fake_curl("printf '200'; exit 0", || {
            let summary =
                build_open_summary_json(&make_target(), Some("/health")).expect("summary json");
            assert_eq!(summary["name"], Value::from("app"));
            assert_eq!(summary["app_url"], Value::from("https://localhost:8080"));
            assert_eq!(
                summary["health_url"],
                Value::from("https://localhost:8080/health")
            );
            assert_eq!(summary["health_status"], Value::from(200));
            assert_eq!(summary["health_ok"], Value::from(true));
        });
    }

    #[test]
    fn build_open_summary_json_marks_unhealthy_status() {
        with_fake_curl("printf '404'; exit 0", || {
            let summary =
                build_open_summary_json(&make_target(), Some("/health")).expect("summary json");
            assert_eq!(summary["health_status"], Value::from(404));
            assert_eq!(summary["health_ok"], Value::from(false));
        });
    }
}
