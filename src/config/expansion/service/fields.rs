//! config expansion service fields module.
//!
//! Contains config expansion service fields logic used by Berth command workflows.

use anyhow::Result;

pub(super) fn value_or_default<T: Copy>(
    value: Option<T>,
    default: Option<T>,
    field: &str,
) -> Result<T> {
    value
        .or(default)
        .ok_or_else(|| anyhow::anyhow!("service is missing required field '{field}'"))
}

pub(super) fn pick_required(
    value: Option<String>,
    default: Option<&str>,
    fallback: String,
) -> String {
    value
        .or_else(|| default.map(ToOwned::to_owned))
        .unwrap_or(fallback)
}

pub(super) fn pick_with_default(
    value: Option<String>,
    default: Option<&str>,
    missing_error: impl FnOnce() -> anyhow::Error,
) -> Result<String> {
    value
        .or_else(|| default.map(ToOwned::to_owned))
        .ok_or_else(missing_error)
}

pub(super) fn merge_opt_owned(value: Option<String>, default: Option<&str>) -> Option<String> {
    value.or_else(|| default.map(ToOwned::to_owned))
}

pub(super) fn merge_opt_copy<T: Copy>(value: Option<T>, default: Option<T>) -> Option<T> {
    value.or(default)
}
