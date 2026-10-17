use crate::config::{Driver, Kind, ServiceConfig};

use super::provider::ShareProvider;

#[test]
fn cloudflare_args_disable_tls_verify_for_https_upstreams() {
    let args = ShareProvider::Cloudflare.command_args("https://localhost:8443");
    assert_eq!(
        args,
        vec![
            "tunnel",
            "--url",
            "https://localhost:8443",
            "--no-tls-verify"
        ]
    );
}

#[test]
fn tailscale_args_use_funnel_command() {
    let args = ShareProvider::Tailscale.command_args("http://127.0.0.1:8080");
    assert_eq!(args, vec!["funnel", "--bg", "8080"]);
}

#[test]
fn expose_args_use_share_command() {
    let args = ShareProvider::Expose.command_args("http://127.0.0.1:8080");
    assert_eq!(args, vec!["share", "http://127.0.0.1:8080"]);
}

#[test]
fn public_url_extraction_matches_provider_patterns() {
    let cloudflare_output = "Available at https://orange-cat.trycloudflare.com";
    let cloudflare_url =
        super::state::extract_provider_url(cloudflare_output, ShareProvider::Cloudflare);
    assert_eq!(
        cloudflare_url,
        Some("https://orange-cat.trycloudflare.com".to_owned())
    );

    let tailscale_output = "funnel active: https://devbox.tail123.ts.net";
    let tailscale_url =
        super::state::extract_provider_url(tailscale_output, ShareProvider::Tailscale);
    assert_eq!(
        tailscale_url,
        Some("https://devbox.tail123.ts.net".to_owned())
    );
}

#[test]
fn local_target_url_uses_loopback_for_unspecified_host() {
    let mut service = app_service();
    service.host = "0.0.0.0".to_owned();
    service.scheme = Some("http".to_owned());

    let url = super::target::local_target_url(&service);
    assert_eq!(url, "http://127.0.0.1:33065");
}

#[test]
fn local_target_url_prefers_https_for_localhost_tls_services() {
    let mut service = app_service();
    service.localhost_tls = true;

    let url = super::target::local_target_url(&service);
    assert_eq!(url, "https://127.0.0.1:33065");
}

fn app_service() -> ServiceConfig {
    ServiceConfig {
        name: "app".to_owned(),
        kind: Kind::App,
        driver: Driver::Frankenphp,
        image: "dunglas/frankenphp:php8.4".to_owned(),
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
        scheme: None,
        domain: Some("acme-api.berth".to_owned()),
        domains: None,
        resolved_domain: None,
        container_port: Some(80),
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
        localhost_tls: false,
        octane: false,
        octane_workers: None,
        octane_max_requests: None,
        php_extensions: None,
        trust_container_ca: false,
        env_mapping: None,
        javascript: None,
        container_name: Some("acme-api-app".to_owned()),
        resolved_container_name: Some("acme-api-app".to_owned()),
        runtime: Default::default(),
    }
}
