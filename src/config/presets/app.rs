//! config presets app module.
//!
//! Contains config presets app logic used by Berth command workflows.

use super::PresetDefaults;

mod defaults;
mod extensions;
mod health;
mod horizon;
mod queue_worker;
mod reverb;
mod scheduler;
mod soketi;

pub(super) const PRESET_NAMES: &[&str] = &[
    "frankenphp",
    "laravel",
    "reverb",
    "horizon",
    "queue-worker",
    "queue",
    "scheduler",
    "dusk",
    "selenium",
    "gotenberg",
    "mailhog",
    "mailpit",
    "rabbitmq",
    "soketi",
];

/// Resolves resolve using configured inputs and runtime state.
pub(super) fn resolve(preset: &str) -> Option<PresetDefaults> {
    match preset {
        "frankenphp" => Some(defaults::frankenphp()),
        "laravel" => Some(defaults::laravel()),
        "reverb" => Some(reverb::defaults()),
        "horizon" => Some(horizon::defaults()),
        "queue-worker" | "queue" => Some(queue_worker::defaults()),
        "scheduler" => Some(scheduler::defaults()),
        "dusk" => Some(defaults::dusk()),
        "selenium" => Some(defaults::selenium()),
        "gotenberg" => Some(defaults::gotenberg()),
        "mailhog" => Some(defaults::mailhog()),
        "mailpit" => Some(defaults::mailpit()),
        "rabbitmq" => Some(defaults::rabbitmq()),
        "soketi" => Some(soketi::defaults()),
        _ => None,
    }
}

/// Returns the default value for health path for preset.
pub(super) fn default_health_path_for_preset(preset: &str) -> Option<&'static str> {
    health::default_health_path_for_preset(preset)
}

/// Returns the default value for health statuses for preset.
pub(super) fn default_health_statuses_for_preset(preset: &str) -> Option<Vec<u16>> {
    health::default_health_statuses_for_preset(preset)
}
