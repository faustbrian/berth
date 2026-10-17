//! env mapping remap module.
//!
//! Contains env mapping remap logic used by Berth command workflows.

use std::collections::HashMap;

pub(super) fn apply_env_remapping(
    vars: &mut HashMap<String, String>,
    mapping: Option<&HashMap<String, String>>,
) {
    let Some(mapping) = mapping else {
        return;
    };

    let mut remapped = HashMap::new();
    for (semantic, env_name) in mapping {
        if let Some(value) = vars.remove(semantic) {
            remapped.insert(env_name.clone(), value);
        }
    }
    vars.extend(remapped);
}
