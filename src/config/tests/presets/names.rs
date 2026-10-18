use super::*;
use std::collections::HashSet;

#[test]
fn preset_names_are_unique_and_include_primary_aliases() {
    let names = preset_names();
    let unique: HashSet<_> = names.iter().copied().collect();

    assert_eq!(names.len(), unique.len());

    for expected in [
        "mongodb",
        "postgres",
        "pg",
        "pgsql",
        "sqlserver",
        "mssql",
        "mysql",
        "mariadb",
        "redis",
        "valkey",
        "dragonfly",
        "memcached",
        "garage",
        "localstack",
        "rustfs",
        "opensearch",
        "elasticsearch",
        "meilisearch",
        "typesense",
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
    ] {
        assert!(names.contains(&expected));
    }
}

#[test]
fn every_preset_pins_a_versioned_image_tag() {
    // A mutable `latest` tag would give parallel agents different images
    // (8.x L018); `2022-latest` style tags still name the major version.
    for name in preset_names() {
        let image = crate::config::presets::preset_defaults(name)
            .expect("preset")
            .image;
        let tag = image
            .rsplit_once(':')
            .map(|(_, tag)| tag)
            .unwrap_or_else(|| panic!("{name} has no image tag: {image}"));
        assert!(!tag.contains('/'), "{name} has no image tag: {image}");
        for mutable in ["latest", "stable", "nightly", "edge"] {
            assert!(
                !tag.starts_with(mutable),
                "{name} uses a mutable tag: {image}"
            );
        }
    }
}
