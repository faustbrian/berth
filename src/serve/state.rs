//! Shared state/data structures used across serve subsystems.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct DerivedImageLock {
    /// Derived-image signature -> image tag cache mapping.
    pub(crate) entries: BTreeMap<String, String>,
}

/// Result of PHP extension verification for a serve target.
#[derive(Debug, Clone)]
pub struct PhpExtensionCheck {
    pub target: String,
    pub image: String,
    pub missing: Vec<String>,
}
