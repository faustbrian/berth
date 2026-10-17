//! Strict berth 9.x project file (`.berth.yaml`) parsing and legacy detection.
//!
//! Berth reads exactly one project file format. Older helm and stackctl files
//! are never read implicitly; they must be converted with `berth config migrate`.

mod legacy;
mod migrate;
pub(crate) mod schema;

pub(crate) use legacy::{LEGACY_CONFIG_FILE_NAMES, legacy_config_error};
#[cfg(test)]
pub(crate) use migrate::convert_legacy_config;
pub(crate) use migrate::{MigratedProjectFile, is_migratable_file_name, migrate_project_file};
pub(crate) use schema::parse_project_file;

/// Canonical project file name.
pub(crate) const PROJECT_FILE_NAME: &str = ".berth.yaml";

/// Schema version accepted by this build.
pub(crate) const PROJECT_SCHEMA_VERSION: u32 = 9;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use migrate::write_test_config_file;
