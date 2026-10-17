//! Renders the complete Caddy configuration from the current route set.
//!
//! The whole document is rebuilt from container labels on every change, so
//! concurrent `up` and `down` calls cannot lose each other's routes: whichever
//! publishes last publishes the complete, current set.

use serde_json::{Value, json};

use super::GatewayPorts;
use super::route::ROUTE_SUFFIX;

/// One published route: requests for `host` go to `upstream` (`container:port`).
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct GatewayRoute {
    pub(crate) host: String,
    pub(crate) upstream: String,
}

/// Renders the gateway configuration for `routes`.
///
/// Every request for a berth host without a route gets a terminal 404, so an
/// unknown or removed instance never looks healthy (8.x FI-56).
#[must_use]
pub(crate) fn render_config(routes: &[GatewayRoute], ports: GatewayPorts) -> Value {
    let mut routes = routes.to_vec();
    routes.sort();
    routes.dedup_by(|left, right| left.host == right.host);

    let mut https_routes: Vec<Value> = routes
        .iter()
        .map(|route| {
            json!({
                "match": [{"host": [route.host]}],
                "handle": [{
                    "handler": "reverse_proxy",
                    "upstreams": [{"dial": route.upstream}],
                    // Keep WebSocket connections alive across reloads.
                    "stream_close_delay": "5m"
                }],
                "terminal": true
            })
        })
        .collect();
    https_routes.push(not_found_route());

    let redirect_target = if ports.https == 443 {
        "https://{http.request.host}{http.request.uri}".to_owned()
    } else {
        format!(
            "https://{{http.request.host}}:{}{{http.request.uri}}",
            ports.https
        )
    };
    let wildcard = format!("*.{ROUTE_SUFFIX}");

    json!({
        "admin": {"listen": "localhost:2019", "config": {"persist": false}},
        "apps": {
            "pki": {"certificate_authorities": {"local": {
                "name": "Berth Local CA",
                "install_trust": false
            }}},
            "tls": {
                "certificates": {"automate": [wildcard]},
                "automation": {"policies": [{
                    "subjects": [wildcard],
                    "issuers": [{"module": "internal"}]
                }]}
            },
            "http": {"servers": {
                // The gateway listens on the same ports inside its container
                // as on the host, so a route URL works unchanged from inside
                // an instance network, where it resolves to the gateway.
                "berth_https": {
                    "listen": [format!(":{}", ports.https)],
                    // Access log for `berth logs --access` (container stderr).
                    "logs": {},
                    "automatic_https": {"disable_redirects": true},
                    "tls_connection_policies": [{}],
                    "routes": https_routes
                },
                "berth_http": {
                    "listen": [format!(":{}", ports.http)],
                    "automatic_https": {"disable": true},
                    "routes": [{
                        "handle": [{
                            "handler": "static_response",
                            "status_code": 308,
                            "headers": {"Location": [redirect_target]}
                        }],
                        "terminal": true
                    }]
                }
            }}
        }
    })
}

fn not_found_route() -> Value {
    json!({
        "handle": [{
            "handler": "static_response",
            "status_code": 404,
            "body": "unknown berth route\n"
        }],
        "terminal": true
    })
}

#[cfg(test)]
mod tests {
    use super::{GatewayRoute, render_config};
    use crate::gateway::GatewayPorts;

    fn route(host: &str, upstream: &str) -> GatewayRoute {
        GatewayRoute {
            host: host.to_owned(),
            upstream: upstream.to_owned(),
        }
    }

    #[test]
    fn route_order_does_not_change_the_document() {
        let ports = GatewayPorts {
            https: 8443,
            http: 8080,
        };
        let a = route("a.berth.localhost", "berth_p_a_app:80");
        let b = route("b.berth.localhost", "berth_p_b_app:80");
        assert_eq!(
            render_config(&[a.clone(), b.clone()], ports),
            render_config(&[b, a], ports)
        );
    }

    #[test]
    fn unknown_hosts_end_in_a_terminal_404() {
        let config = render_config(
            &[route("a.berth.localhost", "c:80")],
            GatewayPorts {
                https: 8443,
                http: 8080,
            },
        );
        let routes = config["apps"]["http"]["servers"]["berth_https"]["routes"]
            .as_array()
            .expect("routes");
        let last = routes.last().expect("terminal route");
        assert_eq!(last["handle"][0]["status_code"], 404);
        assert!(last.get("match").is_none(), "the catch-all has no matcher");
    }

    #[test]
    fn redirects_carry_a_non_default_https_port() {
        let config = render_config(
            &[],
            GatewayPorts {
                https: 8443,
                http: 8080,
            },
        );
        let location = &config["apps"]["http"]["servers"]["berth_http"]["routes"][0]["handle"][0]["headers"]
            ["Location"][0];
        assert_eq!(
            location,
            "https://{http.request.host}:8443{http.request.uri}"
        );
    }
}
