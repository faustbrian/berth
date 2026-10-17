//! HTTP health-check polling for served app targets.

use anyhow::Result;
use std::collections::HashSet;
use std::thread;
use std::time::Duration;

use crate::cli::support::run_curl_command_with_body;
use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

mod body;
mod observation;
mod url;

use observation::HealthProbeObservation;
use url::health_url_for_target;

/// Polls target health endpoint until success or timeout.
///
/// Accepts configured status allowlists; otherwise treats any 2xx response as
/// healthy. Some drivers (for example Gotenberg) also require body validation.
pub(crate) fn wait_until_http_healthy(
    target: &ServiceConfig,
    timeout_secs: u64,
    interval_secs: u64,
    health_path: Option<&str>,
) -> Result<()> {
    let health_url = health_url_for_target(target, health_path)?;
    let accepted_statuses: HashSet<u16> = target
        .health_statuses
        .clone()
        .unwrap_or_default()
        .into_iter()
        .collect();
    output::event(
        &target.name,
        LogLevel::Info,
        &format!("Waiting for health check at {health_url}"),
        Persistence::Persistent,
    );
    let started = std::time::Instant::now();

    let timeout_details = loop {
        let observation = run_curl_command_with_body(&health_url)
            .map(|result| HealthProbeObservation::from_output(&result))
            .unwrap_or_else(HealthProbeObservation::missing_output);

        if observation.is_healthy(target, &accepted_statuses) {
            let code = observation
                .status()
                .map(|status| status.to_string())
                .unwrap_or_else(|| String::from("healthy"));
            output::event(
                &target.name,
                LogLevel::Success,
                &format!("Health check passed at {health_url} ({code})"),
                Persistence::Persistent,
            );
            return Ok(());
        }

        if started.elapsed() >= Duration::from_secs(timeout_secs) {
            break observation.describe_for_target(target);
        }

        thread::sleep(Duration::from_secs(interval_secs));
    };

    anyhow::bail!(
        "app service '{}' did not become healthy at {} within {}s ({})",
        target.name,
        health_url,
        timeout_secs,
        timeout_details
    );
}

#[cfg(test)]
mod tests {
    use super::wait_until_http_healthy;
    use crate::cli::support::with_curl_command;
    use crate::config::{Driver, Kind, ServiceConfig};
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_target(driver: Driver) -> ServiceConfig {
        ServiceConfig {
            name: "app".to_owned(),
            kind: Kind::App,
            driver,
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

    fn with_fake_curl<F, T>(output: &str, test: F) -> T
    where
        F: FnOnce() -> T,
    {
        let bin_dir = PathBuf::from("/tmp").join(format!(
            "berth-serve-health-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&bin_dir).expect("temp dir");
        let curl = bin_dir.join("curl");
        let mut file = fs::File::create(&curl).expect("create fake curl");
        writeln!(file, "#!/bin/sh\nprintf '{}'", output).expect("write script");
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&curl).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&curl, perms).expect("chmod");
        }

        let command = curl.to_string_lossy().to_string();
        let result = with_curl_command(&command, || test());
        fs::remove_dir_all(&bin_dir).ok();
        result
    }

    fn with_fake_curl_script<F, T>(script: &str, test: F) -> T
    where
        F: FnOnce() -> T,
    {
        let bin_dir = PathBuf::from("/tmp").join(format!(
            "berth-serve-health-script-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&bin_dir).expect("temp dir");
        let curl = bin_dir.join("curl");
        let mut file = fs::File::create(&curl).expect("create fake curl");
        writeln!(file, "#!/bin/sh\n{script}").expect("write script");
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&curl).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&curl, perms).expect("chmod");
        }

        let command = curl.to_string_lossy().to_string();
        let result = with_curl_command(&command, || test());
        fs::remove_dir_all(&bin_dir).ok();
        result
    }

    #[test]
    fn wait_until_http_healthy_succeeds_on_healthy_code() {
        with_fake_curl("body\n200", || {
            wait_until_http_healthy(&make_target(Driver::Frankenphp), 1, 1, Some("/up"))
                .expect("healthy");
        });
    }

    #[test]
    fn wait_until_http_healthy_tolerates_custom_accepted_codes() {
        let mut target = make_target(Driver::Frankenphp);
        target.health_statuses = Some(vec![302]);

        with_fake_curl("body\n302", || {
            wait_until_http_healthy(&target, 1, 1, Some("/up")).expect("allowed status accepted");
        });
    }

    #[test]
    fn wait_until_http_healthy_times_out_when_unhealthy() {
        with_fake_curl("body\n500", || {
            let error =
                wait_until_http_healthy(&make_target(Driver::Frankenphp), 0, 1, Some("/up"))
                    .expect_err("timeout expected");
            assert!(error.to_string().contains("did not become healthy"));
        });
    }

    #[test]
    fn wait_until_http_healthy_reports_last_unhealthy_response_on_timeout() {
        with_fake_curl("bad gateway\n502", || {
            let error =
                wait_until_http_healthy(&make_target(Driver::Frankenphp), 0, 1, Some("/up"))
                    .expect_err("timeout expected");
            let message = error.to_string();
            assert!(message.contains("did not become healthy"));
            assert!(message.contains("last status: 502"));
            assert!(message.contains("last body: bad gateway"));
        });
    }

    #[test]
    fn wait_until_http_healthy_rejects_bad_body_for_gotenberg() {
        let mut target = make_target(Driver::Gotenberg);
        target.health_path = Some("/health/ready".to_owned());
        with_fake_curl("status\":\"down\n200", || {
            let error = wait_until_http_healthy(&target, 0, 1, Some("/health/ready"))
                .expect_err("gotenberg body rejected");
            assert!(error.to_string().contains("did not become healthy"));
        });
    }

    #[test]
    fn wait_until_http_healthy_preserves_response_body_for_gotenberg() {
        let mut target = make_target(Driver::Gotenberg);
        target.health_path = Some("/health".to_owned());
        with_fake_curl_script(
            r#"
for arg in "$@"; do
  if [ "$arg" = "/dev/null" ]; then
    printf '200'
    exit 0
  fi
done
printf '{"status":"up"}\n200'
"#,
            || {
                wait_until_http_healthy(&target, 1, 1, Some("/health"))
                    .expect("gotenberg body should be available");
            },
        );
    }
}
