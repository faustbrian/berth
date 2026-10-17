//! config presets app health module.
//!
//! Contains config presets app health logic used by Berth command workflows.

/// Returns the default value for health path for preset.
pub(super) fn default_health_path_for_preset(preset: &str) -> Option<&'static str> {
    match preset {
        "frankenphp" | "reverb" | "soketi" => Some("/"),
        "laravel" => Some("/up"),
        "dusk" | "selenium" => Some("/wd/hub/status"),
        "gotenberg" => Some("/health"),
        "mailhog" | "mailpit" => Some("/"),
        _ => None,
    }
}

/// Returns the default value for health statuses for preset.
pub(super) fn default_health_statuses_for_preset(preset: &str) -> Option<Vec<u16>> {
    match preset {
        "frankenphp" | "reverb" | "soketi" | "laravel" | "dusk" | "selenium" | "gotenberg"
        | "mailhog" | "mailpit" => Some(vec![200]),
        _ => None,
    }
}
