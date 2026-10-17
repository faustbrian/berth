//! env mapping module.
//!
//! Contains env mapping logic used by Berth command workflows.

use std::collections::HashMap;

use crate::config::ServiceConfig;

mod drivers;
mod remap;

use drivers::base_map_for_driver;
use remap::apply_env_remapping;

/// Applies optional semantic-to-env-name remapping to a variable map.
pub(crate) fn apply_mapping(
    vars: &mut HashMap<String, String>,
    mapping: Option<&HashMap<String, String>>,
) {
    apply_env_remapping(vars, mapping);
}

/// Builds laravel env map for command execution.
pub(super) fn build_laravel_env_map(service: &ServiceConfig) -> HashMap<String, String> {
    let mut vars = base_map_for_driver(service);
    apply_mapping(&mut vars, service.env_mapping.as_ref());
    vars
}
