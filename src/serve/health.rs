//! HTTP health-check polling for served app targets.

use anyhow::Result;
use std::collections::HashSet;
use std::thread;
use std::time::Duration;

use crate::cli::support::run_curl_command_with_body;
use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

mod body;
mod framework_boot;
mod observation;
mod url;

pub(crate) use framework_boot::wait_until_laravel_boots;
use observation::HealthProbeObservation;
use url::health_url_for_target;

/// Polls the target's health endpoint until it is healthy.
///
/// Accepts configured status allowlists; otherwise treats any 2xx response as
/// healthy. Some drivers (for example Gotenberg) also require body validation.
/// Without `timeout_secs` it waits as long as the container runs and the app
/// is still starting; it fails at once when the container stops or the app
/// answers with what waiting cannot change (a 3xx, 4xx or 5xx other than the
/// 502/503/504 a proxy or a starting server gives, or 408/425/429), unless
/// the target accepts that status.
pub(crate) fn wait_until_http_healthy(
    target: &ServiceConfig,
    timeout_secs: Option<u64>,
    interval_secs: u64,
    health_path: Option<&str>,
) -> Result<()> {
    let health_url = health_url_for_target(target, health_path)?;
    // A dry run starts nothing, so there is nothing to probe.
    if crate::docker::is_dry_run() {
        output::event(
            &target.name,
            LogLevel::Info,
            &format!("[dry-run] Wait for health check at {health_url}"),
            Persistence::Transient,
        );
        return Ok(());
    }
    let accepted_statuses: HashSet<u16> = target
        .health_statuses
        .clone()
        .unwrap_or_default()
        .into_iter()
        .collect();
    // A service with neither a route nor a host port (Gotenberg, Selenium)
    // is probed inside its instance network through the gateway.
    let in_network =
        !crate::gateway::has_gateway_routes(target) && !crate::network::is_published(target);
    let probe_path = health_path
        .or(target.health_path.as_deref())
        .unwrap_or("/up")
        .to_owned();
    let health_url = if in_network {
        format!(
            "http://{}:{}{probe_path} (inside {})",
            target.name,
            target.resolved_container_port(),
            target.runtime.network.as_deref().unwrap_or("its network")
        )
    } else {
        health_url
    };
    output::event(
        &target.name,
        LogLevel::Info,
        &format!("Waiting for health check at {health_url}"),
        Persistence::Persistent,
    );
    let started = std::time::Instant::now();
    let container = target.container_name()?;
    let timeout_details = loop {
        crate::parallel::ensure_no_sibling_failed()?;
        match crate::docker::container_state(&container) {
            crate::docker::ContainerState::Running | crate::docker::ContainerState::Unknown => {}
            crate::docker::ContainerState::NotRunning(status) => anyhow::bail!(
                "app service '{}' stopped while starting (container {container} is {status}); \
                 see why with {}, fix it and run {} again",
                target.name,
                crate::followup::logs(target),
                crate::followup::for_service(target, "up")
            ),
            crate::docker::ContainerState::Missing => anyhow::bail!(
                "app service '{}' has no container {container}: it was removed while berth \
                 waited for it (another `berth down`?); run {} again",
                target.name,
                crate::followup::for_service(target, "up")
            ),
        }
        let result = if in_network {
            crate::gateway::http_probe_output(target, &probe_path)
        } else {
            run_curl_command_with_body(&health_url)
                .ok_or_else(|| anyhow::anyhow!("curl produced no output"))
        };
        let observation = match result {
            Ok(output) => HealthProbeObservation::from_output(&output),
            // Starting or attaching the gateway failed: retrying cannot help.
            Err(error) if in_network => return Err(error),
            Err(_) => HealthProbeObservation::missing_output(),
        };

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

        if observation
            .status()
            .is_some_and(|status| is_definitive_failure(status, &accepted_statuses))
        {
            let status = observation.status().unwrap_or_default();
            let fix = if status >= 500 {
                format!("see the error with {}", crate::followup::logs(target))
            } else {
                format!(
                    "point `health_path` at a route that answers 2xx, or add {status} to \
                     `health_statuses`"
                )
            };
            anyhow::bail!(
                "app service '{}' answered {status} at {health_url} ({}); {fix}",
                target.name,
                observation.describe_for_target(target)
            );
        }

        if let Some(timeout_secs) = timeout_secs
            && started.elapsed() >= Duration::from_secs(timeout_secs)
        {
            break (timeout_secs, observation.describe_for_target(target));
        }

        thread::sleep(Duration::from_secs(interval_secs));
    };

    anyhow::bail!(
        "app service '{}' did not become healthy at {} within {}s ({}); see what it is doing \
         with {}, give it longer, or point `health_path` at a faster route",
        target.name,
        health_url,
        timeout_details.0,
        timeout_details.1,
        crate::followup::logs(target)
    );
}

/// An answer a running app keeps giving: a redirect (to a login page), a
/// client error (a missing health route, an auth wall) or a server error.
/// 502, 503 and 504 come from a proxy or a server still starting, and 408,
/// 425 and 429 ask to come back later, so only those are worth waiting out.
fn is_definitive_failure(status: u16, accepted_statuses: &HashSet<u16>) -> bool {
    (300..=599).contains(&status)
        && !matches!(status, 502..=504 | 408 | 425 | 429)
        && !accepted_statuses.contains(&status)
}

#[cfg(test)]
mod tests {
    use super::wait_until_http_healthy;
    use crate::cli::support::with_curl_command;
    use crate::config::{Driver, Kind, ServiceConfig};
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;

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

    /// A `docker` whose `inspect` reports the app container in `status`.
    fn fake_docker(bin_dir: &std::path::Path, status: &str) -> String {
        let docker = bin_dir.join("docker");
        fs::write(&docker, format!("#!/bin/sh\necho {status}\n")).expect("write fake docker");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&docker, fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        docker.to_string_lossy().to_string()
    }

    fn with_fake_curl<F, T>(output: &str, test: F) -> T
    where
        F: FnOnce() -> T,
    {
        let bin_dir = PathBuf::from("/tmp").join(format!(
            "berth-serve-health-{}",
            crate::test_support::unique_stamp()
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
        let docker = fake_docker(&bin_dir, "running");
        let result = crate::docker::with_dry_run_state(false, || {
            crate::docker::with_docker_command(&docker, || with_curl_command(&command, test))
        });
        fs::remove_dir_all(&bin_dir).ok();
        result
    }

    fn with_fake_curl_script<F, T>(script: &str, test: F) -> T
    where
        F: FnOnce() -> T,
    {
        let bin_dir = PathBuf::from("/tmp").join(format!(
            "berth-serve-health-script-{}",
            crate::test_support::unique_stamp()
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
        let docker = fake_docker(&bin_dir, "running");
        let result = crate::docker::with_dry_run_state(false, || {
            crate::docker::with_docker_command(&docker, || with_curl_command(&command, test))
        });
        fs::remove_dir_all(&bin_dir).ok();
        result
    }

    #[test]
    fn wait_until_http_healthy_succeeds_on_healthy_code() {
        with_fake_curl("body\n200", || {
            wait_until_http_healthy(&make_target(Driver::Frankenphp), None, 1, Some("/up"))
                .expect("healthy");
        });
    }

    #[test]
    fn wait_until_http_healthy_tolerates_custom_accepted_codes() {
        let mut target = make_target(Driver::Frankenphp);
        target.health_statuses = Some(vec![302]);

        with_fake_curl("body\n302", || {
            wait_until_http_healthy(&target, None, 1, Some("/up"))
                .expect("allowed status accepted");
        });
    }

    #[test]
    fn a_server_error_fails_the_wait_at_once_without_a_cap() {
        with_fake_curl("Server Error\n500", || {
            let error =
                wait_until_http_healthy(&make_target(Driver::Frankenphp), None, 1, Some("/up"))
                    .expect_err("a 500 is not worth waiting out");
            assert!(error.to_string().contains("answered 500"), "{error}");
        });
    }

    #[test]
    fn a_missing_health_route_fails_the_wait_at_once_without_a_cap() {
        with_fake_curl("Not Found\n404", || {
            let error =
                wait_until_http_healthy(&make_target(Driver::Frankenphp), None, 1, Some("/up"))
                    .expect_err("a 404 never turns into a 2xx");
            assert!(error.to_string().contains("answered 404"), "{error}");
        });
    }

    #[test]
    fn a_redirect_fails_the_wait_at_once_unless_accepted() {
        with_fake_curl("Redirecting to /login\n302", || {
            let error =
                wait_until_http_healthy(&make_target(Driver::Frankenphp), None, 1, Some("/"))
                    .expect_err("a redirect never turns into a 2xx");
            assert!(error.to_string().contains("answered 302"), "{error}");
        });
    }

    #[test]
    fn a_starting_app_is_waited_out_without_a_cap() {
        let counter = crate::docker::unique_test_path(
            crate::test_support::temp_root().join("serve-health-starting"),
        );
        let script = format!(
            "n=$(cat '{c}' 2>/dev/null || echo 0); n=$((n + 1)); echo $n > '{c}'\n\
             if [ $n -le 2 ]; then printf 'starting\\n503'; else printf 'ok\\n200'; fi",
            c = counter.display()
        );
        with_fake_curl_script(&script, || {
            wait_until_http_healthy(&make_target(Driver::Frankenphp), None, 1, Some("/up"))
                .expect("a 503 is waited out until the app answers");
        });
    }

    #[test]
    fn a_stopped_container_fails_the_wait_at_once_without_a_cap() {
        let bin_dir = crate::docker::unique_test_path(
            crate::test_support::temp_root().join("serve-health-stopped"),
        );
        fs::create_dir_all(&bin_dir).expect("temp dir");
        let docker = fake_docker(&bin_dir, "exited");
        let error = crate::docker::with_dry_run_state(false, || {
            crate::docker::with_docker_command(&docker, || {
                wait_until_http_healthy(&make_target(Driver::Frankenphp), None, 1, Some("/up"))
            })
        })
        .expect_err("a stopped container never becomes healthy");
        assert!(
            error.to_string().contains("stopped while starting"),
            "{error}"
        );
    }

    #[test]
    fn wait_until_http_healthy_reports_last_unhealthy_response_on_timeout() {
        with_fake_curl("bad gateway\n502", || {
            let error =
                wait_until_http_healthy(&make_target(Driver::Frankenphp), Some(0), 1, Some("/up"))
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
            let error = wait_until_http_healthy(&target, Some(0), 1, Some("/health/ready"))
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
                wait_until_http_healthy(&target, None, 1, Some("/health"))
                    .expect("gotenberg body should be available");
            },
        );
    }
}
