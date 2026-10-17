//! Caddyfile rendering and port parsing helpers.

use anyhow::{Context, Result};
use std::path::Path;

use super::{CaddyPorts, CaddyState};

/// Renders the full Caddyfile for all active routes.
///
/// The global block sets fixed HTTP/HTTPS ports and file-based access logging.
pub(super) fn render_caddyfile(
    state: &CaddyState,
    ports: CaddyPorts,
    access_log_path: &Path,
) -> String {
    let mut caddyfile = String::new();
    caddyfile.push_str("{\n");
    caddyfile.push_str("    auto_https disable_redirects\n");
    caddyfile.push_str(&format!("    http_port {}\n", ports.http));
    caddyfile.push_str(&format!("    https_port {}\n", ports.https));
    caddyfile.push_str("    log {\n");
    caddyfile.push_str(&format!(
        "        output file {}\n",
        access_log_path.display()
    ));
    caddyfile.push_str("        format console\n");
    caddyfile.push_str("    }\n");
    caddyfile.push_str("}\n\n");

    for (domain, upstream) in &state.routes {
        caddyfile.push_str(&format!("{domain} {{\n"));
        caddyfile.push_str(&format!("    reverse_proxy {upstream} {{\n"));
        caddyfile.push_str("        header_up X-Forwarded-Proto https\n");
        caddyfile.push_str("        header_up X-Forwarded-Port {http.request.port}\n");
        caddyfile.push_str("        header_up X-Forwarded-Host {host}\n");
        caddyfile.push_str("    }\n");
        caddyfile.push_str("    tls internal\n");
        caddyfile.push_str("}\n\n");
    }

    caddyfile
}

// TODO: resolve from config
/// Resolves Caddy ports from `BERTH_CADDY_{HTTP,HTTPS}_PORT`.
///
/// Defaults to `80/443` when env vars are unset.
pub(super) fn resolve_caddy_ports() -> Result<CaddyPorts> {
    let http = resolve_port_env("BERTH_CADDY_HTTP_PORT", 80)?;
    let https = resolve_port_env("BERTH_CADDY_HTTPS_PORT", 443)?;

    Ok(CaddyPorts { http, https })
}

fn resolve_port_env(var_name: &'static str, default: u16) -> Result<u16> {
    std::env::var(var_name)
        .ok()
        .map_or(Ok(default), parse_port_env(var_name))
}

/// Returns a parser for validating non-zero `u16` port values.
fn parse_port_env(var_name: &'static str) -> impl FnOnce(String) -> Result<u16> {
    move |raw| {
        let port = raw
            .parse::<u16>()
            .with_context(|| format!("{var_name} must be a valid u16 port number"))?;
        if port == 0 {
            anyhow::bail!("{var_name} must be >= 1");
        }
        Ok(port)
    }
}

/// Formats a public HTTPS URL, omitting the port for default `443`.
pub(super) fn served_url(domain: &str, https_port: u16) -> String {
    if https_port == 443 {
        return format!("https://{domain}");
    }

    format!("https://{domain}:{https_port}")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::CaddyPorts;
    use super::super::CaddyState;
    use super::parse_port_env;
    use super::render_caddyfile;
    use super::resolve_port_env;

    #[test]
    fn rendered_caddyfile_lists_routes_and_shared_settings() {
        let mut routes = CaddyState::default();
        routes
            .routes
            .insert("acme.test".to_owned(), "127.0.0.1:8080".to_owned());

        let rendered = render_caddyfile(
            &routes,
            CaddyPorts {
                http: 81,
                https: 444,
            },
            Path::new("/tmp/access.log"),
        );

        assert!(rendered.contains("http_port 81"));
        assert!(rendered.contains("https_port 444"));
        assert!(rendered.contains("acme.test {"));
        assert!(rendered.contains("reverse_proxy 127.0.0.1:8080"));
        assert!(rendered.contains("tls internal"));
    }

    #[test]
    fn resolve_caddy_ports_defaults_when_env_is_unset() {
        let ports = resolve_port_env("BERTH_CADDY_MISSING_HTTP_PORT", 80).expect("missing http");
        assert_eq!(ports, 80);
    }

    #[test]
    fn resolve_caddy_ports_parses_custom_values() {
        let parser = parse_port_env("BERTH_TEST_HTTPS_PORT");
        assert_eq!(parser("8443".to_owned()).expect("custom"), 8443);
    }

    #[test]
    fn resolve_caddy_ports_rejects_zero_port_values() {
        let parser = parse_port_env("BERTH_TEST_HTTP_PORT");
        assert!(parser("0".to_owned()).is_err());
    }
}
