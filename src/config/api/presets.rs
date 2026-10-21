//! config api presets module.
//!
//! Contains config api presets logic used by Berth command workflows.

use anyhow::Result;

use super::super::{RawServiceConfig, ServiceConfig, expansion, presets};

/// Returns all supported preset names.
#[must_use]
pub fn preset_names() -> Vec<&'static str> {
    presets::preset_names()
}

/// Resolves a preset into a default service config preview.
///
/// # Errors
///
/// Returns an error if the preset is unknown.
pub fn preset_preview(preset: &str) -> Result<ServiceConfig> {
    expansion::expand_raw_service(RawServiceConfig {
        preset: Some(preset.to_owned()),
        name: None,
        kind: None,
        driver: None,
        image: None,
        host: None,
        port: None,
        database: None,
        username: None,
        password: None,
        bucket: None,
        access_key: None,
        secret_key: None,
        api_key: None,
        region: None,
        scheme: None,
        domain: None,
        domains: None,
        routes: None,
        container_port: None,
        smtp_port: None,
        volumes: None,
        env: None,
        command: None,
        depends_on: None,
        seed_file: None,
        hook: Vec::new(),
        health_path: None,
        health_statuses: None,
        restart: None,
        localhost_tls: None,
        octane: None,
        octane_workers: None,
        octane_max_requests: None,
        php_extensions: None,
        trust_container_ca: None,
        env_mapping: None,
        javascript: None,
        container_name: None,
        publish: None,
        cpus: None,
        memory: None,
        isolation: None,
    })
}
