//! config expansion service selection module.
//!
//! Contains config expansion service selection logic used by Berth command workflows.

use anyhow::{Result, anyhow};

use super::super::super::{Driver, RawServiceConfig, presets};
use super::fields::{pick_required, pick_with_default};

/// Resolves name and image using configured inputs and runtime state.
pub(super) fn resolve_name_and_image(
    raw: &RawServiceConfig,
    defaults: Option<&presets::PresetDefaults>,
    driver: Driver,
) -> Result<(String, String)> {
    let name = pick_required(
        raw.name.clone(),
        defaults.and_then(|d| d.name),
        format!("{driver:?}").to_lowercase(),
    );
    let image = pick_with_default(raw.image.clone(), defaults.map(|d| d.image), || {
        anyhow!(
            "service '{name}' has no `preset` to supply an image; set `image:` (for example \
             `image: postgres:18`) or a `preset`"
        )
    })?;

    Ok((name, image))
}
