use super::super::inferred_app_env;
use super::helpers::svc;
use crate::config::{Config, Driver, Kind};

#[test]
fn inferred_app_env_sets_app_and_asset_urls_for_localhost_tls() {
    let mut app = svc("app", Kind::App, Driver::Frankenphp, 8443);
    app.localhost_tls = true;

    let config = Config {
        schema_version: 1,
        project_type: crate::config::ProjectType::Project,
        container_prefix: Some("app".to_owned()),
        domain_strategy: None,
        service: vec![app],
        swarm: vec![],
    };

    let vars = inferred_app_env(&config);

    assert_eq!(
        vars.get("APP_URL"),
        Some(&"https://localhost:8443".to_owned())
    );
    assert_eq!(
        vars.get("ASSET_URL"),
        Some(&"https://localhost:8443".to_owned())
    );
}

#[test]
fn inferred_app_env_sets_app_and_asset_urls_for_domain() {
    let mut app = svc("app", Kind::App, Driver::Frankenphp, 8080);
    app.domain = Some("acme-api.berth".to_owned());

    let config = Config {
        schema_version: 1,
        project_type: crate::config::ProjectType::Project,
        container_prefix: Some("app".to_owned()),
        domain_strategy: None,
        service: vec![app],
        swarm: vec![],
    };

    let vars = inferred_app_env(&config);

    assert_eq!(
        vars.get("APP_URL"),
        Some(&"https://acme-api.berth".to_owned())
    );
    assert_eq!(
        vars.get("ASSET_URL"),
        Some(&"https://acme-api.berth".to_owned())
    );
    assert_eq!(
        vars.get("LIVEWIRE_TEMPORARY_FILE_UPLOAD_DISK"),
        Some(&"local".to_owned())
    );
}

#[test]
fn inferred_app_env_uses_primary_domain_from_domains_list() {
    let mut app = svc("app", Kind::App, Driver::Frankenphp, 8080);
    app.domains = Some(vec!["primary.berth".to_owned(), "alt.berth".to_owned()]);

    let config = Config {
        schema_version: 1,
        project_type: crate::config::ProjectType::Project,
        container_prefix: Some("app".to_owned()),
        domain_strategy: None,
        service: vec![app],
        swarm: vec![],
    };

    let vars = inferred_app_env(&config);

    assert_eq!(
        vars.get("APP_URL"),
        Some(&"https://primary.berth".to_owned())
    );
    assert_eq!(
        vars.get("ASSET_URL"),
        Some(&"https://primary.berth".to_owned())
    );
}

#[test]
fn inferred_app_env_sets_default_app_name_for_app_targets() {
    let app = svc("app", Kind::App, Driver::Frankenphp, 8080);

    let config = Config {
        schema_version: 1,
        project_type: crate::config::ProjectType::Project,
        container_prefix: Some("app".to_owned()),
        domain_strategy: None,
        service: vec![app],
        swarm: vec![],
    };

    let vars = inferred_app_env(&config);

    assert_eq!(vars.get("APP_NAME"), Some(&"Laravel".to_owned()));
}
