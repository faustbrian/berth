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
fn inferred_app_env_sets_app_and_asset_urls_for_the_instance_route() {
    let mut app = svc("app", Kind::App, Driver::Frankenphp, 8080);
    app.domain = Some("acme-api.berth.localhost".to_owned());
    app.resolved_domain = Some("a-1-app.berth.localhost".to_owned());
    app.runtime.gateway_https_port = Some(8443);

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
        Some(&"https://a-1-app.berth.localhost:8443".to_owned())
    );
    assert_eq!(
        vars.get("ASSET_URL"),
        Some(&"https://a-1-app.berth.localhost:8443".to_owned())
    );
    assert_eq!(
        vars.get("LIVEWIRE_TEMPORARY_FILE_UPLOAD_DISK"),
        Some(&"local".to_owned())
    );
}

#[test]
fn app_urls_use_the_unique_instance_route_before_authored_domains() {
    let mut app = svc("app", Kind::App, Driver::Frankenphp, 8080);
    app.domains = Some(vec![
        "primary.berth.localhost".to_owned(),
        "alt.berth.localhost".to_owned(),
    ]);
    app.resolved_domain = Some("a-1-app.berth.localhost".to_owned());
    app.runtime.gateway_https_port = Some(8443);

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
        Some(&"https://a-1-app.berth.localhost:8443".to_owned())
    );
    assert_eq!(
        vars.get("ASSET_URL"),
        Some(&"https://a-1-app.berth.localhost:8443".to_owned())
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

#[test]
fn apps_without_gateway_routes_get_their_direct_url() {
    // An `--env` namespace has no routes; its app keeps 7.0.0's direct URL.
    let app = svc("app", Kind::App, Driver::Frankenphp, 18080);
    let expected = crate::serve::local_url(&app);
    let config = Config {
        schema_version: 1,
        project_type: crate::config::ProjectType::Project,
        container_prefix: Some("app".to_owned()),
        domain_strategy: None,
        service: vec![app],
        swarm: vec![],
    };

    let vars = inferred_app_env(&config);
    assert_eq!(vars.get("APP_URL"), Some(&expected));
    assert!(expected.ends_with(":18080"), "{expected}");
}

#[test]
fn a_worker_listed_before_the_web_app_never_sets_app_url() {
    let mut worker = svc("queue-worker", Kind::App, Driver::Frankenphp, 18081);
    worker.command = Some(vec![
        "php".to_owned(),
        "artisan".to_owned(),
        "queue:work".to_owned(),
    ]);
    let mut app = svc("app", Kind::App, Driver::Frankenphp, 18080);
    app.resolved_domain = Some("a-1-app.berth.localhost".to_owned());
    app.runtime.gateway_https_port = Some(8443);
    let config = Config {
        schema_version: 1,
        project_type: crate::config::ProjectType::Project,
        container_prefix: Some("app".to_owned()),
        domain_strategy: None,
        service: vec![worker, app],
        swarm: vec![],
    };

    let vars = inferred_app_env(&config);
    assert_eq!(
        vars.get("APP_URL"),
        Some(&"https://a-1-app.berth.localhost:8443".to_owned())
    );
}

fn routed_config(gateway_https_port: Option<u16>) -> Config {
    let mut app = svc("app", Kind::App, Driver::Frankenphp, 8080);
    app.routes = Some(std::collections::BTreeMap::from([
        ("ee".to_owned(), "APP_EE_URL".to_owned()),
        ("fi".to_owned(), "APP_FI_URL".to_owned()),
    ]));
    if let Some(port) = gateway_https_port {
        app.resolved_domain = Some("a-1-app.berth.localhost".to_owned());
        app.runtime.gateway_https_port = Some(port);
        app.runtime.extra_routes = ["ee", "fi"]
            .into_iter()
            .map(|label| crate::config::ExtraRoute {
                label: label.to_owned(),
                env: format!("APP_{}_URL", label.to_ascii_uppercase()),
                host: format!("a-1-app-{label}.berth.localhost"),
            })
            .collect();
    }
    Config {
        schema_version: 1,
        project_type: crate::config::ProjectType::Project,
        container_prefix: Some("app".to_owned()),
        domain_strategy: None,
        service: vec![app],
        swarm: vec![],
    }
}

#[test]
fn routes_inject_their_urls_formatted_like_app_url() {
    let vars = inferred_app_env(&routed_config(Some(8443)));
    assert_eq!(
        vars.get("APP_URL"),
        Some(&"https://a-1-app.berth.localhost:8443".to_owned())
    );
    assert_eq!(
        vars.get("APP_EE_URL"),
        Some(&"https://a-1-app-ee.berth.localhost:8443".to_owned())
    );
    assert_eq!(
        vars.get("APP_FI_URL"),
        Some(&"https://a-1-app-fi.berth.localhost:8443".to_owned())
    );

    let vars = inferred_app_env(&routed_config(Some(443)));
    assert_eq!(
        vars.get("APP_EE_URL"),
        Some(&"https://a-1-app-ee.berth.localhost".to_owned())
    );
}

#[test]
fn unrouted_apps_leave_their_route_variables_to_the_app() {
    let vars = inferred_app_env(&routed_config(None));
    assert!(vars.contains_key("APP_URL"));
    assert_eq!(vars.get("APP_EE_URL"), None);
    assert_eq!(vars.get("APP_FI_URL"), None);
}

#[test]
fn apps_with_routes_get_no_asset_url_so_assets_follow_the_site_host() {
    let vars = inferred_app_env(&routed_config(Some(8443)));
    assert_eq!(vars.get("ASSET_URL"), None);
    assert_eq!(
        vars.get("APP_URL"),
        Some(&"https://a-1-app.berth.localhost:8443".to_owned())
    );
    let vars = inferred_app_env(&routed_config(None));
    assert_eq!(vars.get("ASSET_URL"), None);
}

fn routed_peer(name: &str, driver: Driver) -> crate::config::ServiceConfig {
    let mut peer = svc(name, Kind::App, driver, 8090);
    peer.resolved_domain = Some(format!("a-1-app-{name}.berth.localhost"));
    peer.runtime.gateway_https_port = Some(8443);
    peer
}

#[test]
fn asset_url_follows_only_the_app_that_sets_app_url() {
    let mut config = routed_config(Some(8443));
    config
        .service
        .push(routed_peer("admin", Driver::Frankenphp));
    config.service.push(routed_peer("mail", Driver::Mailhog));

    let vars = inferred_app_env(&config);
    assert_eq!(
        vars.get("APP_URL"),
        Some(&"https://a-1-app.berth.localhost:8443".to_owned())
    );
    assert_eq!(vars.get("ASSET_URL"), None, "{vars:?}");

    let host = crate::env::host_env(&config);
    assert_eq!(
        host.get("APP_URL"),
        Some(&"https://a-1-app.berth.localhost:8443".to_owned())
    );
    assert_eq!(host.get("ASSET_URL"), None, "{host:?}");
}
