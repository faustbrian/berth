//! Reverb app target env inference.

use std::collections::HashMap;

use crate::config::ServiceConfig;

use super::super::{insert_if_absent, runtime_host_for_app, runtime_port_for_app};

/// Applies inferred Reverb env keys.
pub(super) fn apply(vars: &mut HashMap<String, String>, service: &ServiceConfig) {
    insert_if_absent(vars, "BROADCAST_CONNECTION", "reverb".to_owned());
    insert_if_absent(vars, "REVERB_HOST", runtime_host_for_app(service));
    insert_if_absent(
        vars,
        "REVERB_PORT",
        runtime_port_for_app(service).to_string(),
    );
    insert_if_absent(vars, "REVERB_SCHEME", service.scheme().to_owned());
}
