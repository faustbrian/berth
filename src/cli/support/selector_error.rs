//! Errors for a `--service`/`--kind`/`--profile` selection that does not
//! resolve to exactly one service.

use crate::config;

/// The selector as typed, for an error naming it.
fn describe(service: Option<&str>, kind: Option<config::Kind>, profile: Option<&str>) -> String {
    let mut parts = Vec::new();
    if let Some(service) = service {
        parts.push(format!("--service {service}"));
    }
    if let Some(kind) = kind {
        // The CLI spells `ObjectStore` as `object-store`.
        parts.push(format!(
            "--kind {}",
            super::kind_name(kind).replace("objectstore", "object-store")
        ));
    }
    if let Some(profile) = profile {
        parts.push(format!("--profile {profile}"));
    }
    if parts.is_empty() {
        "the selection".to_owned()
    } else {
        parts.join(" ")
    }
}

fn names<'a>(services: impl Iterator<Item = &'a config::ServiceConfig>) -> String {
    let names: Vec<&str> = services.map(|service| service.name.as_str()).collect();
    if names.is_empty() {
        "none".to_owned()
    } else {
        names.join(", ")
    }
}

/// No service, or no app service when `apps_only`, matched the selector.
pub(crate) fn nothing_matched(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    profile: Option<&str>,
    apps_only: bool,
) -> anyhow::Error {
    let selector = describe(service, kind, profile);
    if apps_only {
        let app_services = || {
            config
                .service
                .iter()
                .filter(|svc| svc.kind == config::Kind::App)
        };
        if app_services().next().is_none() {
            return anyhow::anyhow!(
                "this project has no app service, and this command runs in one; add one under \
                 `services:` in .berth.yaml (for example `app: {{preset: laravel}}`)"
            );
        }
        let apps = names(app_services());
        // Only a non-app service explains itself; an app filtered out by
        // `--kind`/`--profile` falls through to the selector message.
        if let Some(named) = service
            .and_then(|name| config.service.iter().find(|svc| svc.name == name))
            .filter(|svc| svc.kind != config::Kind::App)
        {
            return anyhow::anyhow!(
                "'{}' is a {} service, but this command runs in an app service (app services: \
                 {apps}); pass `--service <app>`",
                named.name,
                super::kind_name(named.kind).replace("objectstore", "object-store")
            );
        }
        return anyhow::anyhow!(
            "{selector} matched no app service (app services: {apps}); pass `--service <app>`"
        );
    }
    anyhow::anyhow!(
        "{selector} matched no service (services: {}); {} lists them",
        names(config.service.iter()),
        crate::followup::command("ls --services")
    )
}

/// Several services matched where one is needed.
pub(crate) fn several_matched<'a>(
    service: Option<&str>,
    kind: Option<config::Kind>,
    profile: Option<&str>,
    matched: impl Iterator<Item = &'a config::ServiceConfig>,
) -> anyhow::Error {
    anyhow::anyhow!(
        "{} matched {}; pick one with `--service <name>`",
        describe(service, kind, profile),
        names(matched)
    )
}

#[cfg(test)]
mod tests {
    use super::{nothing_matched, several_matched};
    use crate::config::{Config, Kind, ServiceConfig};

    fn service(name: &str, kind: &str, driver: &str) -> ServiceConfig {
        serde_json::from_value(serde_json::json!({
            "name": name, "kind": kind, "driver": driver,
            "image": "image", "host": "127.0.0.1", "port": 8000
        }))
        .expect("service")
    }

    fn config() -> Config {
        Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: Some("shop".to_owned()),
            domain_strategy: None,
            service: vec![
                service("db", "database", "postgres"),
                service("app", "app", "frankenphp"),
                service("worker", "app", "frankenphp"),
            ],
            swarm: vec![],
        }
    }

    #[test]
    fn a_non_app_service_is_named_with_the_app_services_to_pick_from() {
        let message = nothing_matched(&config(), Some("db"), None, None, true).to_string();
        assert!(message.contains("'db' is a database service"), "{message}");
        assert!(message.contains("app services: app, worker"), "{message}");
    }

    #[test]
    fn an_empty_selection_names_the_selector_and_what_exists() {
        let message = nothing_matched(&config(), None, Some(Kind::Cache), None, false).to_string();
        assert!(
            message.contains("--kind cache matched no service"),
            "{message}"
        );
        assert!(message.contains("db, app, worker"), "{message}");
    }

    #[test]
    fn an_app_removed_by_kind_blames_the_selector_not_the_app() {
        let message =
            nothing_matched(&config(), Some("app"), Some(Kind::Database), None, true).to_string();
        assert!(
            message.contains("--service app --kind database matched no app service"),
            "{message}"
        );
        assert!(!message.contains("is a app service"), "{message}");
    }

    #[test]
    fn several_matches_are_listed() {
        let config = config();
        let message =
            several_matched(None, Some(Kind::App), None, config.service[1..].iter()).to_string();
        assert!(
            message.contains("--kind app matched app, worker"),
            "{message}"
        );
        assert!(message.contains("--service <name>"), "{message}");
    }
}
