//! A secret file whose contents are not a valid secret.

use std::path::PathBuf;

/// Raised only when a secret file exists and holds an invalid value, so
/// callers add recovery advice for corruption alone and never for an
/// unreadable file or a full disk.
#[derive(Debug)]
pub(crate) struct InvalidSecret {
    pub(crate) path: PathBuf,
}

impl std::fmt::Display for InvalidSecret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} does not hold a valid secret",
            self.path.display()
        )
    }
}

impl std::error::Error for InvalidSecret {}

/// Adds `advice` to `error` when it is an [`InvalidSecret`]; any other error
/// is returned unchanged.
pub(crate) fn advise_on_invalid(
    error: anyhow::Error,
    advice: impl FnOnce() -> String,
) -> anyhow::Error {
    if let Some(invalid) = error.downcast_ref::<InvalidSecret>() {
        // Lead with the path, so "the file" in the advice cannot be read
        // as any other file the message names.
        let advice = format!("{}: {}", invalid.path.display(), advice());
        error.context(advice)
    } else {
        error
    }
}

/// Whether `error` comes from a secret file holding an invalid value.
#[must_use]
pub(crate) fn is_invalid_secret(error: &anyhow::Error) -> bool {
    error.downcast_ref::<InvalidSecret>().is_some()
}
