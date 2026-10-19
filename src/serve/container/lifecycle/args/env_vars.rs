//! Environment and host-gateway argument helpers for serve run args.

use std::collections::BTreeSet;
use std::collections::HashMap;
use std::net::IpAddr;

use crate::config::ServiceConfig;

pub(super) fn append_env_args(
    run_args: &mut Vec<String>,
    target: &ServiceConfig,
    injected_env: &HashMap<String, String>,
    inject_server_name: bool,
) {
    for (key, value) in container_env_pairs(target, injected_env, inject_server_name) {
        append_env_binding(run_args, &key, &value);
    }
}

/// The `-e` pairs an app container receives, in order: injected values,
/// then the app's explicit `env` (except an `http` URL override of an
/// injected `https` URL), then FrankenPHP's `SERVER_NAME`. Docker keeps the
/// last value of a repeated key, so this order is also the precedence.
pub(crate) fn container_env_pairs(
    target: &ServiceConfig,
    injected_env: &HashMap<String, String>,
    inject_server_name: bool,
) -> Vec<(String, String)> {
    let mut pairs: Vec<(String, String)> = injected_env
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if let Some(env_vars) = &target.env {
        for (key, value) in env_vars {
            if should_block_insecure_url_override(key, value, injected_env) {
                continue;
            }
            pairs.push((key.clone(), value.clone()));
        }
    }
    if inject_server_name {
        pairs.push(("SERVER_NAME".to_owned(), ":80".to_owned()));
    }
    pairs
}

pub(super) fn append_host_gateway_mapping(
    run_args: &mut Vec<String>,
    target: &ServiceConfig,
    injected_env: &HashMap<String, String>,
) {
    let host_gateway_alias = crate::docker::host_gateway_alias();
    let injected_requests_gateway = injected_env
        .values()
        .any(|value| value.contains(host_gateway_alias));
    let explicit_requests_gateway = target.env.as_ref().is_some_and(|values| {
        values
            .values()
            .any(|value| value.contains(host_gateway_alias))
    });

    if (target.uses_host_gateway_alias() || injected_requests_gateway || explicit_requests_gateway)
        && let Some(mapping) = crate::docker::host_gateway_mapping()
    {
        run_args.push("--add-host".to_owned());
        run_args.push(mapping.to_owned());
    }

    append_domain_host_gateway_mappings(run_args, target, injected_env);
}

fn should_block_insecure_url_override(
    key: &str,
    value: &str,
    injected_env: &HashMap<String, String>,
) -> bool {
    if !matches!(key, "APP_URL" | "ASSET_URL") {
        return false;
    }

    let Some(inferred_value) = injected_env.get(key) else {
        return false;
    };

    inferred_value.starts_with("https://") && !value.starts_with("https://")
}

fn append_env_binding(run_args: &mut Vec<String>, key: &str, value: &str) {
    run_args.push("-e".to_owned());
    run_args.push(format!("{key}={value}"));
}

fn append_domain_host_gateway_mappings(
    run_args: &mut Vec<String>,
    target: &ServiceConfig,
    injected_env: &HashMap<String, String>,
) {
    let Some(mapping) = crate::docker::host_gateway_mapping() else {
        return;
    };

    let host_gateway_alias = crate::docker::host_gateway_alias();
    let mut local_domains = BTreeSet::new();
    let mut local_suffixes = BTreeSet::new();

    // The instance's own routes resolve to the gateway through its network
    // aliases for resolvers that consult DNS; only other instances' routes
    // need the host gateway. libcurl (PHP's HTTP clients) resolves every
    // `*.localhost` name to loopback itself and never sees either, so
    // in-container clients use service names or swarm endpoints instead.
    for domain in target.resolved_domains() {
        if let Some(host) = normalize_hostname(domain)
            && let Some(suffix) = hostname_suffix(&host)
        {
            local_suffixes.insert(suffix.to_owned());
        }
    }

    for value in injected_env.values() {
        if let Some(host) = host_from_env_value(value) {
            let allow_from_env =
                hostname_suffix(&host).is_some_and(|suffix| local_suffixes.contains(suffix));
            if allow_from_env
                && !crate::gateway::is_instance_route(target, &host)
                && !crate::gateway::route_hosts(target).contains(&host)
                && host != host_gateway_alias
                && !is_literal_ip(&host)
                && host != "localhost"
            {
                local_domains.insert(host);
            }
        }
    }

    for domain in local_domains {
        run_args.push("--add-host".to_owned());
        run_args.push(format!("{domain}:{}", mapping_target(mapping)));
    }
}

fn mapping_target(mapping: &str) -> &str {
    mapping
        .split_once(':')
        .map_or(mapping, |(_, target)| target)
}

fn host_from_env_value(value: &str) -> Option<String> {
    if let Some(host) = extract_url_host(value) {
        return normalize_hostname(&host);
    }

    normalize_hostname(value)
}

fn extract_url_host(value: &str) -> Option<String> {
    let (_scheme, remainder) = value.split_once("://")?;
    let authority = remainder
        .split('/')
        .next()
        .unwrap_or_default()
        .split('?')
        .next()
        .unwrap_or_default()
        .split('#')
        .next()
        .unwrap_or_default();
    let without_userinfo = authority.rsplit('@').next().unwrap_or_default();

    if without_userinfo.is_empty() {
        return None;
    }

    if without_userinfo.starts_with('[') {
        return without_userinfo
            .split(']')
            .next()
            .map(|host| host.trim_start_matches('[').to_owned());
    }

    let host = without_userinfo.split(':').next().unwrap_or_default();
    if host.is_empty() {
        return None;
    }

    Some(host.to_owned())
}

fn normalize_hostname(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains('/') || trimmed.contains('?') || trimmed.contains('#') {
        return None;
    }
    if trimmed.contains("://") {
        return None;
    }
    if let Some(stripped) = trimmed.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
        return (!stripped.is_empty()).then(|| stripped.to_ascii_lowercase());
    }

    let host = trimmed.split(':').next().unwrap_or_default();
    if host.is_empty() {
        return None;
    }

    Some(host.to_ascii_lowercase())
}

fn is_literal_ip(host: &str) -> bool {
    host.parse::<IpAddr>().is_ok()
}

fn hostname_suffix(host: &str) -> Option<&str> {
    host.rsplit('.').next().filter(|suffix| *suffix != host)
}

#[cfg(test)]
mod tests {
    use super::append_host_gateway_mapping;
    use crate::config::{Driver, Kind, ServiceConfig};
    use std::collections::HashMap;

    fn app_service() -> ServiceConfig {
        ServiceConfig {
            name: "app".to_owned(),
            kind: Kind::App,
            driver: Driver::Frankenphp,
            image: "dunglas/frankenphp:php8.5".to_owned(),
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
            domain: Some("shipit-api.berth.localhost".to_owned()),
            domains: Some(vec!["alt-api.berth.localhost".to_owned()]),
            resolved_domain: None,
            routes: None,
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
            container_name: Some("shipit-api-app".to_owned()),
            resolved_container_name: Some("shipit-api-app".to_owned()),
            runtime: Default::default(),
        }
    }

    #[test]
    fn adds_host_gateway_alias_and_domain_mappings() {
        let mut run_args = Vec::new();
        let service = app_service();
        let injected = HashMap::from([(
            "POSTAL_API_BASE_URL".to_owned(),
            "https://postal-api.berth.localhost".to_owned(),
        )]);

        append_host_gateway_mapping(&mut run_args, &service, &injected);
        let rendered = run_args.join(" ");

        assert!(rendered.contains("--add-host host.docker.internal:host-gateway"));
        // Own routes resolve to the gateway through network aliases.
        assert!(
            !rendered.contains("shipit-api.berth.localhost:"),
            "{rendered}"
        );
        assert!(!rendered.contains("alt-api.berth.localhost:"), "{rendered}");
        assert!(rendered.contains("--add-host postal-api.berth.localhost:host-gateway"));
    }

    #[test]
    fn does_not_map_non_local_or_ip_hosts_from_env_values() {
        let mut run_args = Vec::new();
        let service = app_service();
        let injected = HashMap::from([
            (
                "EXTERNAL_URL".to_owned(),
                "https://api.stripe.com".to_owned(),
            ),
            ("IP_URL".to_owned(), "http://127.0.0.1:9000".to_owned()),
            ("LOCALHOST".to_owned(), "localhost".to_owned()),
        ]);

        append_host_gateway_mapping(&mut run_args, &service, &injected);
        let rendered = run_args.join(" ");

        assert!(!rendered.contains("api.stripe.com:host-gateway"));
        assert!(!rendered.contains("127.0.0.1:host-gateway"));
        assert!(!rendered.contains("localhost:host-gateway"));
    }
}
