//! config presets preset defaults module.
//!
//! Contains config presets preset defaults logic used by Berth command workflows.

use super::{Driver, Kind};

#[derive(Debug, Clone)]
pub(in crate::config) struct PresetDefaults {
    pub(in crate::config) name: Option<&'static str>,
    pub(in crate::config) kind: Kind,
    pub(in crate::config) driver: Driver,
    pub(in crate::config) image: &'static str,
    pub(in crate::config) host: &'static str,
    pub(in crate::config) port: Option<u16>,
    pub(in crate::config) database: Option<&'static str>,
    pub(in crate::config) username: Option<&'static str>,
    pub(in crate::config) password: Option<&'static str>,
    pub(in crate::config) bucket: Option<&'static str>,
    pub(in crate::config) access_key: Option<&'static str>,
    pub(in crate::config) secret_key: Option<&'static str>,
    pub(in crate::config) api_key: Option<&'static str>,
    pub(in crate::config) region: Option<&'static str>,
    pub(in crate::config) scheme: Option<&'static str>,
    pub(in crate::config) container_port: Option<u16>,
    pub(in crate::config) smtp_port: Option<u16>,
    pub(in crate::config) octane: bool,
    pub(in crate::config) octane_workers: Option<u16>,
    pub(in crate::config) octane_max_requests: Option<u32>,
    pub(in crate::config) php_extensions: Option<Vec<String>>,
    pub(in crate::config) volumes: Option<Vec<String>>,
    pub(in crate::config) command: Option<Vec<String>>,
    pub(in crate::config) forced_env: Option<Vec<(&'static str, &'static str)>>,
    pub(in crate::config) trust_container_ca: bool,
}

impl PresetDefaults {
    pub(super) fn base(kind: Kind, driver: Driver, image: &'static str) -> Self {
        Self {
            name: None,
            kind,
            driver,
            image,
            host: "127.0.0.1",
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
            container_port: None,
            smtp_port: None,
            octane: false,
            octane_workers: None,
            octane_max_requests: None,
            php_extensions: None,
            volumes: None,
            command: None,
            forced_env: None,
            trust_container_ca: false,
        }
    }
}
