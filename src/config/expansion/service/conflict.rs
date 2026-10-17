//! config expansion service conflict module.
//!
//! Contains config expansion service conflict logic used by Berth command workflows.

use anyhow::Result;

use super::super::super::{Driver, Kind, RawServiceConfig};

/// Validates preset conflicts and reports actionable failures.
pub(super) fn validate_preset_conflicts(
    raw: &RawServiceConfig,
    default_kind: Option<Kind>,
    default_driver: Option<Driver>,
    kind: Kind,
    driver: Driver,
) -> Result<()> {
    if default_kind.is_some_and(|value| value != kind) {
        anyhow::bail!(
            "preset kind conflict for service '{}'",
            raw.name.clone().unwrap_or_default()
        );
    }
    if default_driver.is_some_and(|value| value != driver) {
        anyhow::bail!(
            "preset driver conflict for service '{}'",
            raw.name.clone().unwrap_or_default()
        );
    }

    Ok(())
}
