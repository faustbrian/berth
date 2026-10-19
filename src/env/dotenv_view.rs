//! Read-only renderings of the managed environment for tools.

use std::collections::{BTreeMap, HashMap};

use crate::config::Config;

/// Renders values as dotenv lines (`KEY="value"`), sorted by key.
#[must_use]
pub(crate) fn render_dotenv(values: &BTreeMap<String, String>) -> String {
    values
        .iter()
        .map(|(key, value)| format!("{key}={}\n", super::files::quote_env_value(value)))
        .collect()
}

/// Endpoint values a host tool can use: the app's route URLs and the
/// connection values of services published on a host port (with their real
/// bound port). Services reachable only inside their instance network are
/// left out rather than given an address that would not answer.
#[must_use]
pub(crate) fn host_env(config: &Config) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    for service in &config.service {
        if let Some(url) = crate::gateway::route_urls(service).into_iter().next()
            && crate::gateway::has_gateway_routes(service)
        {
            // The app URL is the web app's, as in the containers: a mail UI
            // or WebSocket server listed first has routes but is not the app.
            let web_app = service.kind == crate::config::Kind::App
                && service.driver == crate::config::Driver::Frankenphp
                && crate::gateway::is_routable(service);
            if web_app && !vars.contains_key("APP_URL") {
                // `ASSET_URL` belongs to the service that sets `APP_URL`.
                if service.routes.is_none() {
                    vars.insert("ASSET_URL".to_owned(), url.clone());
                }
                vars.insert("APP_URL".to_owned(), url);
            }
            if let Some(port) = service.runtime.gateway_https_port {
                for route in crate::gateway::extra_routes(service) {
                    vars.entry(route.env.clone())
                        .or_insert_with(|| crate::gateway::https_origin(&route.host, port));
                }
            }
            continue;
        }
        if service.kind == crate::config::Kind::App || !crate::network::is_published(service) {
            continue;
        }
        let mut reachable = service.clone();
        reachable.host = "127.0.0.1".to_owned();
        if let Some(port) = crate::network::host_binding_port(service) {
            reachable.port = port;
        }
        for (key, value) in super::mapping::build_laravel_env_map(&reachable) {
            vars.entry(key).or_insert(value);
        }
    }
    vars
}
