//! `.localhost` resolution self-test.

use std::net::ToSocketAddrs;

/// Returns whether `host` resolves only to loopback addresses on this
/// machine. Every `*.localhost` name should (RFC 6761); berth relies on it
/// instead of editing `/etc/hosts`.
#[must_use]
pub(crate) fn resolves_to_loopback(host: &str) -> bool {
    match format!("{host}:443").to_socket_addrs() {
        Ok(addrs) => {
            let addrs: Vec<_> = addrs.collect();
            !addrs.is_empty() && addrs.iter().all(|addr| addr.ip().is_loopback())
        }
        Err(_) => false,
    }
}
