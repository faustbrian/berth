//! config presets module.
//!
//! Contains config presets logic used by Berth command workflows.

use anyhow::Result;

use super::{Driver, Kind};

mod app;
mod cache;
mod database;
mod dropped;
mod object_store;
mod preset_defaults;
mod search;

pub(in crate::config) use dropped::dropped_preset;
pub(in crate::config) use preset_defaults::PresetDefaults;

pub(super) fn preset_defaults(preset: &str) -> Result<PresetDefaults> {
    if let Some(reason) = dropped_preset(preset) {
        anyhow::bail!("{reason}");
    }
    if let Some(defaults) = database::resolve(preset) {
        return Ok(defaults);
    }

    if let Some(defaults) = cache::resolve(preset) {
        return Ok(defaults);
    }

    if let Some(defaults) = object_store::resolve(preset) {
        return Ok(defaults);
    }

    if let Some(defaults) = search::resolve(preset) {
        return Ok(defaults);
    }

    if let Some(defaults) = app::resolve(preset) {
        return Ok(defaults);
    }

    anyhow::bail!(
        "unknown preset '{preset}'; presets: {} (`berth preset show <name>` shows its defaults)",
        preset_names().join(", ")
    )
}

#[must_use]
pub(super) fn preset_names() -> Vec<&'static str> {
    let mut names = Vec::new();
    names.extend_from_slice(database::PRESET_NAMES);
    names.extend_from_slice(cache::PRESET_NAMES);
    names.extend_from_slice(object_store::PRESET_NAMES);
    names.extend_from_slice(search::PRESET_NAMES);
    names.extend_from_slice(app::PRESET_NAMES);
    names
}

/// Returns the default value for health path for preset.
pub(super) fn default_health_path_for_preset(preset: &str) -> Option<&'static str> {
    app::default_health_path_for_preset(preset)
}

/// Returns the default value for health statuses for preset.
pub(super) fn default_health_statuses_for_preset(preset: &str) -> Option<Vec<u16>> {
    app::default_health_statuses_for_preset(preset)
}
