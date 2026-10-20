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
            "service '{}' sets `kind: {}`, but preset '{}' is kind {}; remove `kind` or choose a \
             preset of that kind (`berth preset list` names them, `berth preset show <name>` shows each one's kind and driver)",
            raw.name.clone().unwrap_or_default(),
            yaml_name(&kind),
            raw.preset.clone().unwrap_or_default(),
            yaml_name(&default_kind.unwrap_or(kind))
        );
    }
    if default_driver.is_some_and(|value| value != driver) {
        anyhow::bail!(
            "service '{}' sets `driver: {}`, but preset '{}' uses driver {}; remove `driver` or \
             choose a preset with that driver (`berth preset list` names them, `berth preset show <name>` shows each one's kind and driver)",
            raw.name.clone().unwrap_or_default(),
            yaml_name(&driver),
            raw.preset.clone().unwrap_or_default(),
            yaml_name(&default_driver.unwrap_or(driver))
        );
    }

    Ok(())
}

/// How a value is spelled in `.berth.yaml`: its serde name.
fn yaml_name(value: &impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_default()
}
