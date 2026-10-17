//! config presets search module.
//!
//! Contains config presets search logic used by Berth command workflows.

use super::{Driver, Kind, PresetDefaults};

pub(super) const PRESET_NAMES: &[&str] =
    &["opensearch", "elasticsearch", "meilisearch", "typesense"];

/// Resolves resolve using configured inputs and runtime state.
pub(super) fn resolve(preset: &str) -> Option<PresetDefaults> {
    match preset {
        "opensearch" => Some(opensearch()),
        "elasticsearch" => Some(elasticsearch()),
        "meilisearch" => Some(meilisearch()),
        "typesense" => Some(typesense()),
        _ => None,
    }
}

fn opensearch() -> PresetDefaults {
    let mut defaults = PresetDefaults::base(
        Kind::Search,
        Driver::Opensearch,
        "opensearchproject/opensearch:latest",
    );
    defaults.name = Some("search");
    defaults.forced_env = Some(vec![
        ("SCOUT_DRIVER", "opensearch"),
        ("discovery.type", "single-node"),
        ("DISABLE_SECURITY_PLUGIN", "true"),
    ]);
    defaults
}

fn elasticsearch() -> PresetDefaults {
    let mut defaults = PresetDefaults::base(
        Kind::Search,
        Driver::Elasticsearch,
        "docker.elastic.co/elasticsearch/elasticsearch:9.4.2",
    );
    defaults.name = Some("search");
    defaults.forced_env = Some(vec![
        ("SCOUT_DRIVER", "elasticsearch"),
        ("discovery.type", "single-node"),
        ("xpack.security.enabled", "false"),
    ]);
    defaults
}

fn meilisearch() -> PresetDefaults {
    let mut defaults = PresetDefaults::base(
        Kind::Search,
        Driver::Meilisearch,
        "getmeili/meilisearch:latest",
    );
    defaults.name = Some("search");
    defaults.api_key = Some("masterKey");
    defaults.forced_env = Some(vec![
        ("SCOUT_DRIVER", "meilisearch"),
        ("MEILISEARCH_KEY", "masterKey"),
    ]);
    defaults
}

fn typesense() -> PresetDefaults {
    let mut defaults = PresetDefaults::base(
        Kind::Search,
        Driver::Typesense,
        "typesense/typesense:0.26.0",
    );
    defaults.name = Some("search");
    defaults.api_key = Some("xyz");
    defaults.forced_env = Some(vec![
        ("SCOUT_DRIVER", "typesense"),
        ("TYPESENSE_API_KEY", "xyz"),
    ]);
    defaults
}
