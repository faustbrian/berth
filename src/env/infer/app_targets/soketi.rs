//! Soketi app target env inference.

use std::collections::HashMap;

use crate::config::ServiceConfig;

use super::super::{insert_if_absent, runtime_host_for_app, runtime_port_for_app};

/// Applies inferred Soketi broadcasting env keys.
pub(super) fn apply(vars: &mut HashMap<String, String>, service: &ServiceConfig) {
    insert_if_absent(vars, "BROADCAST_CONNECTION", "pusher".to_owned());
    insert_if_absent(vars, "PUSHER_APP_ID", "app-id".to_owned());
    insert_if_absent(vars, "PUSHER_APP_KEY", "app-key".to_owned());
    insert_if_absent(vars, "PUSHER_APP_SECRET", "app-secret".to_owned());
    insert_if_absent(vars, "PUSHER_HOST", runtime_host_for_app(service));
    insert_if_absent(
        vars,
        "PUSHER_PORT",
        runtime_port_for_app(service).to_string(),
    );
    insert_if_absent(vars, "PUSHER_SCHEME", service.scheme().to_owned());
}
