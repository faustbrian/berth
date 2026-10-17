//! config raw service module.
//!
//! Contains config raw service logic used by Berth command workflows.

use serde::Deserialize;
use std::collections::HashMap;

use super::super::{Driver, Kind, RestartPolicy};
use super::RawServiceHook;
use crate::javascript::JavaScriptToolchain;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawServiceConfig {
    #[serde(default)]
    pub preset: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub kind: Option<Kind>,
    #[serde(default)]
    pub driver: Option<Driver>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub bucket: Option<String>,
    #[serde(default)]
    pub access_key: Option<String>,
    #[serde(default)]
    pub secret_key: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub scheme: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub domains: Option<Vec<String>>,
    #[serde(default)]
    pub container_port: Option<u16>,
    #[serde(default)]
    pub smtp_port: Option<u16>,
    #[serde(default)]
    pub volumes: Option<Vec<String>>,
    #[serde(
        default,
        deserialize_with = "crate::config::unique_map::optional_unique_hash_map"
    )]
    pub env: Option<HashMap<String, String>>,
    #[serde(default)]
    pub command: Option<Vec<String>>,
    #[serde(default)]
    pub depends_on: Option<Vec<String>>,
    #[serde(default)]
    pub seed_file: Option<String>,
    #[serde(default)]
    pub hook: Vec<RawServiceHook>,
    #[serde(default)]
    pub health_path: Option<String>,
    #[serde(default)]
    pub health_statuses: Option<Vec<u16>>,
    #[serde(default)]
    pub restart: Option<RestartPolicy>,
    #[serde(default)]
    pub localhost_tls: Option<bool>,
    #[serde(default)]
    pub octane: Option<bool>,
    #[serde(default)]
    pub octane_workers: Option<u16>,
    #[serde(default)]
    pub octane_max_requests: Option<u32>,
    #[serde(default)]
    pub php_extensions: Option<Vec<String>>,
    #[serde(default)]
    pub trust_container_ca: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::config::unique_map::optional_unique_hash_map"
    )]
    pub env_mapping: Option<HashMap<String, String>>,
    #[serde(default)]
    pub javascript: Option<JavaScriptToolchain>,
    #[serde(default)]
    pub container_name: Option<String>,
}
