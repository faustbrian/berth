//! Detection of helm and stackctl project files that berth no longer reads.

use std::path::Path;

/// Project files from earlier releases, in the order they are reported.
pub(crate) const LEGACY_CONFIG_FILE_NAMES: [&str; 5] = [
    ".helm.toml",
    ".helm.yaml",
    ".stackctl.toml",
    ".stackctl.yaml",
    ".berth.toml",
];

/// Builds the error reported when a legacy file stands where `expected` (a
/// berth 9 file name such as `.berth.yaml` or `.berth.testing.yaml`) belongs.
#[must_use]
pub(crate) fn legacy_config_error(legacy_path: &Path, expected: &str) -> anyhow::Error {
    let dir = legacy_path.parent().unwrap_or_else(|| Path::new("."));
    anyhow::anyhow!(
        "found legacy project file {} where berth 9 expects {expected}; berth 9 never reads \
         helm, stackctl or TOML project files. Run `berth config migrate` in {} to convert it",
        legacy_path.display(),
        dir.display()
    )
}
