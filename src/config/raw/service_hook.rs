//! config raw service hook module.
//!
//! Contains raw service hook configuration for TOML parsing.

use serde::Deserialize;

use super::super::{HookOnError, HookPhase};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawServiceHook {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub phase: Option<HookPhase>,
    #[serde(default)]
    pub run: Option<RawHookRun>,
    #[serde(default)]
    pub on_error: Option<HookOnError>,
    #[serde(default)]
    pub timeout_sec: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum RawHookRun {
    Exec { argv: Vec<String> },
    Script { path: String },
}
