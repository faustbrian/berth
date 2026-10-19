//! Captures the most recent HTTP health probe observation.

use std::collections::HashSet;
use std::process::Output;

use crate::config::ServiceConfig;

use super::body::body_health_is_ok;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct HealthProbeObservation {
    status: Option<u16>,
    body: Option<String>,
    display_body: Option<String>,
    error: Option<String>,
}

impl HealthProbeObservation {
    pub(super) fn from_output(output: &Output) -> Self {
        if !output.status.success() {
            return Self {
                status: None,
                body: None,
                display_body: None,
                error: Some(compact_text(
                    &String::from_utf8_lossy(&output.stderr),
                    &String::from_utf8_lossy(&output.stdout),
                )),
            };
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let mut lines = stdout.lines().collect::<Vec<_>>();
        let Some(code_line) = lines.pop() else {
            return Self {
                status: None,
                body: None,
                display_body: None,
                error: Some(String::from("health probe returned no status code")),
            };
        };

        let body = lines.join("\n");
        match code_line.trim().parse::<u16>() {
            Ok(status) => Self {
                status: Some(status),
                body: raw_body(&body),
                display_body: normalize_body(&body),
                error: None,
            },
            Err(_) => Self {
                status: None,
                body: raw_body(&stdout),
                display_body: normalize_body(&stdout),
                error: Some(format!(
                    "health probe returned invalid status code: {}",
                    code_line.trim()
                )),
            },
        }
    }

    pub(super) fn missing_output() -> Self {
        Self {
            status: None,
            body: None,
            display_body: None,
            error: Some(String::from("health probe command produced no output")),
        }
    }

    pub(super) fn is_healthy(
        &self,
        target: &ServiceConfig,
        accepted_statuses: &HashSet<u16>,
    ) -> bool {
        self.status.is_some_and(|status| {
            ((accepted_statuses.is_empty() && (200..=299).contains(&status))
                || accepted_statuses.contains(&status))
                && body_health_is_ok(target, self.body.as_deref().unwrap_or_default())
        })
    }

    pub(super) fn status(&self) -> Option<u16> {
        self.status
    }

    pub(super) fn describe(&self) -> String {
        let mut parts = Vec::new();

        if let Some(status) = self.status {
            parts.push(format!("last status: {status}"));
        }
        if let Some(body) = &self.display_body {
            parts.push(format!("last body: {body}"));
        }
        if let Some(error) = &self.error {
            parts.push(format!("last probe error: {error}"));
        }

        if parts.is_empty() {
            String::from("no health probe details captured")
        } else {
            parts.join("; ")
        }
    }

    pub(super) fn describe_for_target(&self, target: &ServiceConfig) -> String {
        let mut description = self.describe();

        if self.status == Some(502) && !target.localhost_tls {
            description.push_str(
                "; reverse proxy returned 502, which usually means the upstream app \
container started but is not serving valid HTTP yet; inspect the app container logs",
            );
        }

        description
    }
}

fn raw_body(body: &str) -> Option<String> {
    let trimmed = body.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn normalize_body(body: &str) -> Option<String> {
    let compact = raw_body(body)?
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if compact.len() <= 160 {
        Some(compact)
    } else {
        Some(format!("{}...", &compact[..157]))
    }
}

fn compact_text(primary: &str, fallback: &str) -> String {
    normalize_body(primary)
        .or_else(|| normalize_body(fallback))
        .unwrap_or_else(|| String::from("curl exited without stderr output"))
}

#[cfg(test)]
mod tests {
    use super::HealthProbeObservation;
    use crate::config::{Driver, Kind, ServiceConfig};
    use std::collections::HashSet;
    use std::process::Command;

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

    #[test]
    fn describe_includes_status_and_body() {
        let output = Command::new("sh")
            .args(["-c", "printf 'bad gateway\\n502'"])
            .output()
            .expect("command output");
        let observation = HealthProbeObservation::from_output(&output);

        assert_eq!(
            observation.describe(),
            "last status: 502; last body: bad gateway"
        );
    }

    #[test]
    fn describe_for_target_explains_reverse_proxy_502() {
        let output = Command::new("sh")
            .args(["-c", "printf 'bad gateway\\n502'"])
            .output()
            .expect("command output");
        let observation = HealthProbeObservation::from_output(&output);
        let mut target = make_target(Driver::Frankenphp);
        target.localhost_tls = false;

        assert!(
            observation
                .describe_for_target(&target)
                .contains("reverse proxy returned 502")
        );
    }

    #[test]
    fn failed_command_reports_probe_error() {
        let output = Command::new("sh")
            .args(["-c", "printf 'connect failed' >&2; exit 7"])
            .output()
            .expect("command output");
        let observation = HealthProbeObservation::from_output(&output);

        assert_eq!(observation.describe(), "last probe error: connect failed");
    }

    #[test]
    fn gotenberg_health_uses_body_validation() {
        let output = Command::new("sh")
            .args(["-c", "printf '{\"status\":\"UP\"}\\n200'"])
            .output()
            .expect("command output");
        let observation = HealthProbeObservation::from_output(&output);

        assert!(observation.is_healthy(&make_target(Driver::Gotenberg), &HashSet::new()));
    }

    #[test]
    fn gotenberg_health_preserves_long_json_body_for_validation() {
        let payload = "{\"status\":\"up\",\"details\":{\"chromium\":{\"status\":\"up\",\"timestamp\":\"2026-03-09T12:15:09.10396747Z\"},\"libreoffice\":{\"status\":\"up\",\"timestamp\":\"2026-03-09T12:15:09.10396747Z\"},\"uno\":{\"status\":\"up\",\"timestamp\":\"2026-03-09T12:15:09.10396747Z\"}}}";
        let output = Command::new("sh")
            .args(["-c", &format!("printf '{}\\n200'", payload)])
            .output()
            .expect("command output");
        let observation = HealthProbeObservation::from_output(&output);

        assert!(observation.is_healthy(&make_target(Driver::Gotenberg), &HashSet::new()));
    }
}
