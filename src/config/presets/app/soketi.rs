//! Soketi app preset defaults.

use super::super::{Driver, Kind, PresetDefaults};

pub(super) fn defaults() -> PresetDefaults {
    let mut defaults = PresetDefaults::base(
        Kind::App,
        Driver::Soketi,
        "quay.io/soketi/soketi:5d188786beaf683aca2115a6247dcdc15c29ac77-16-debian",
    );
    defaults.name = Some("soketi");
    defaults.container_port = Some(6001);
    defaults.forced_env = Some(vec![
        ("SOKETI_DEFAULT_APP_ID", "app-id"),
        ("SOKETI_DEFAULT_APP_KEY", "app-key"),
        ("SOKETI_DEFAULT_APP_SECRET", "app-secret"),
    ]);
    defaults
}
