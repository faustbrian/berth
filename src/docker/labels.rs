//! docker labels module.
//!
//! Contains Berth Docker label keys and values used by Berth command workflows.

use crate::config::Kind;

pub(crate) const LABEL_MANAGED: &str = "dev.berth.managed";
pub(crate) const LABEL_SERVICE: &str = "dev.berth.service";
pub(crate) const LABEL_KIND: &str = "dev.berth.kind";
pub(crate) const LABEL_CONTAINER: &str = "dev.berth.container";
pub(crate) const VALUE_MANAGED_TRUE: &str = "true";

pub(crate) fn kind_label_value(kind: Kind) -> &'static str {
    match kind {
        Kind::Database => "database",
        Kind::Cache => "cache",
        Kind::ObjectStore => "object-store",
        Kind::Search => "search",
        Kind::App => "app",
    }
}
