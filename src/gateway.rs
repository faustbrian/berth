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
pub(crate) use ports::{GatewayPorts, https_origin};
pub(crate) use probe::{
    ProbeSetupError, http_probe_output, http_status, probe_without_starting_gateway, tcp_open,
};
pub(crate) use publish::{
    NETWORK_LABEL, PublishMode, ROUTE_HOSTS_LABEL, ROUTE_OWNED_LABEL, ROUTE_PORT_LABEL,
    detach_network, ensure_route_unclaimed, is_attached, publish_routes, remove_gateway_if_unused,
};
pub(crate) use resolve::resolves_to_loopback;
pub(crate) use route::{
    extra_route_host, extra_routes, has_gateway_routes, is_instance_route, is_routable,
    owned_route_hosts, route_host, route_hosts, route_urls, validate_authored_domain,
};
