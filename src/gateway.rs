//! The berth gateway: one Caddy container that terminates TLS for every
//! instance route on `127.0.0.1` and probes service health from inside the
//! instance networks.

mod container;
mod ports;
mod probe;
mod publish;
mod render;
mod resolve;
mod route;

pub(crate) use container::gateway_name;
pub(crate) use ports::GatewayPorts;
pub(crate) use probe::{
    ProbeSetupError, http_probe_output, http_status, probe_without_starting_gateway, tcp_open,
};
pub(crate) use publish::{
    NETWORK_LABEL, PublishMode, ROUTE_HOSTS_LABEL, ROUTE_PORT_LABEL, detach_network,
    ensure_route_unclaimed, is_attached, publish_routes,
};
pub(crate) use resolve::resolves_to_loopback;
pub(crate) use route::{
    has_gateway_routes, is_instance_route, is_routable, route_host, route_hosts, route_urls,
    validate_authored_domain,
};
