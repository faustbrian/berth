//! config presets cache module.
//!
//! Contains config presets cache logic used by Berth command workflows.

use super::{Driver, Kind, PresetDefaults};

pub(super) const PRESET_NAMES: &[&str] = &["redis", "valkey", "dragonfly", "memcached"];

/// Resolves resolve using configured inputs and runtime state.
pub(super) fn resolve(preset: &str) -> Option<PresetDefaults> {
    match preset {
        "redis" => Some(redis()),
        "valkey" => Some(valkey()),
        "dragonfly" => Some(dragonfly()),
        "memcached" => Some(memcached()),
        _ => None,
    }
}

fn redis() -> PresetDefaults {
    let mut defaults = PresetDefaults::base(Kind::Cache, Driver::Redis, "redis:7-alpine");
    defaults.name = Some("redis");
    defaults
}

fn valkey() -> PresetDefaults {
    let mut defaults = PresetDefaults::base(Kind::Cache, Driver::Valkey, "valkey/valkey:8");
    defaults.name = Some("valkey");
    defaults
}

fn dragonfly() -> PresetDefaults {
    let mut defaults = PresetDefaults::base(
        Kind::Cache,
        Driver::Dragonfly,
        "docker.dragonflydb.io/dragonflydb/dragonfly:v1.39.0",
    );
    defaults.name = Some("dragonfly");
    defaults
}

fn memcached() -> PresetDefaults {
    let mut defaults = PresetDefaults::base(Kind::Cache, Driver::Memcached, "memcached:1.6-alpine");
    defaults.name = Some("memcached");
    defaults
}
