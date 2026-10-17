use super::super::caddy::{render_caddyfile, served_url};
use super::super::hosts::hosts_file_has_domain;
use super::super::{CaddyPorts, CaddyState};
use std::collections::BTreeMap;
use std::path::Path;

#[test]
fn caddyfile_contains_domain_and_upstream() {
    let mut routes = BTreeMap::new();
    routes.insert("donkey.berth".to_owned(), "127.0.0.1:8080".to_owned());
    let state = CaddyState { routes };
    let ports = CaddyPorts {
        http: 80,
        https: 443,
    };

    let rendered = render_caddyfile(&state, ports, Path::new("/tmp/access.log"));
    assert!(rendered.contains("donkey.berth"));
    assert!(rendered.contains("reverse_proxy 127.0.0.1:8080"));
    assert!(rendered.contains("header_up X-Forwarded-Proto https"));
    assert!(rendered.contains("header_up X-Forwarded-Port {http.request.port}"));
    assert!(rendered.contains("header_up X-Forwarded-Host {host}"));
    assert!(rendered.contains("tls internal"));
}

#[test]
fn hosts_parser_detects_domain() {
    let content = "127.0.0.1 localhost donkey.berth\n";
    assert!(hosts_file_has_domain(content, "donkey.berth"));
}

#[test]
fn hosts_parser_ignores_comments() {
    let content = "# 127.0.0.1 donkey.berth\n127.0.0.1 localhost\n";
    assert!(!hosts_file_has_domain(content, "donkey.berth"));
}

#[test]
fn served_url_hides_default_https_port() {
    assert_eq!(served_url("acme-api.berth", 443), "https://acme-api.berth");
}

#[test]
fn served_url_includes_non_default_https_port() {
    assert_eq!(
        served_url("acme-api.berth", 8443),
        "https://acme-api.berth:8443"
    );
}
