//! config presets object store module.
//!
//! Contains config presets object store logic used by Berth command workflows.

use super::{Driver, Kind, PresetDefaults};

pub(super) const PRESET_NAMES: &[&str] = &["minio", "garage", "localstack", "rustfs"];

/// Resolves resolve using configured inputs and runtime state.
pub(super) fn resolve(preset: &str) -> Option<PresetDefaults> {
    match preset {
        "minio" => Some(minio()),
        "garage" => Some(garage()),
        "localstack" => Some(localstack()),
        "rustfs" => Some(rustfs()),
        _ => None,
    }
}

fn minio() -> PresetDefaults {
    // MinIO's last community release pinned by 8.x. MinIO no longer serves
    // images from Docker Hub or Quay (checked 2026-10-18: access denied), so
    // this preset needs the image already present locally; `rustfs` and
    // `garage` are S3-compatible replacements.
    let mut defaults = PresetDefaults::base(
        Kind::ObjectStore,
        Driver::Minio,
        "minio/minio:RELEASE.2025-09-07T16-13-09Z",
    );
    defaults.name = Some("s3");
    defaults.bucket = Some("media");
    defaults.access_key = Some("minio");
    defaults.secret_key = Some("miniosecret");
    defaults.region = Some("us-east-1");
    defaults
}

fn rustfs() -> PresetDefaults {
    let mut defaults = PresetDefaults::base(
        Kind::ObjectStore,
        Driver::Rustfs,
        "rustfs/rustfs:1.0.0-beta.2",
    );
    defaults.name = Some("s3");
    defaults.bucket = Some("media");
    defaults.access_key = Some("minio");
    defaults.secret_key = Some("miniosecret");
    defaults.region = Some("us-east-1");
    defaults
}

fn garage() -> PresetDefaults {
    let mut defaults =
        PresetDefaults::base(Kind::ObjectStore, Driver::Garage, "dxflrs/garage:v2.1.0");
    defaults.name = Some("s3");
    defaults.bucket = Some("media");
    defaults.access_key = Some("minio");
    defaults.secret_key = Some("miniosecret");
    defaults.region = Some("us-east-1");
    defaults
}

fn localstack() -> PresetDefaults {
    let mut defaults = PresetDefaults::base(
        Kind::ObjectStore,
        Driver::Localstack,
        "localstack/localstack:4.12.0",
    );
    defaults.name = Some("s3");
    defaults.bucket = Some("media");
    defaults.access_key = Some("test");
    defaults.secret_key = Some("test");
    defaults.region = Some("us-east-1");
    defaults
}
