//! config runtime env ports module.
//!
//! Contains config runtime env ports logic used by Berth command workflows.

use anyhow::Result;

pub(super) fn runtime_env_port_offset(env_name: &str) -> u16 {
    if env_name.starts_with("testing-") {
        return 0;
    }

    if env_name == "testing" {
        return 1_000;
    }

    let checksum = env_name
        .bytes()
        .fold(0_u32, |acc, byte| acc.saturating_add(u32::from(byte)));
    let band = (checksum % 20) + 1;
    (band * 1_000) as u16
}

pub(super) fn shift_port(
    port: u16,
    offset: u16,
    service_name: &str,
    field_name: &str,
) -> Result<u16> {
    port.checked_add(offset).ok_or_else(|| {
        anyhow::anyhow!(
            "cannot apply env port offset (+{offset}) to {field_name} \
             for service '{service_name}' (base port {port})"
        )
    })
}
