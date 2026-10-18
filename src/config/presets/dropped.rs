//! Presets berth no longer offers, with the reason and the replacement.

/// Why the `minio` preset is gone: MinIO's community edition is no longer
/// fully open source and no longer publishes images. Both replacements speak
/// the S3 API.
const MINIO_REMOVED: &str = "preset 'minio' was removed: MinIO's community \
     edition is no longer fully open source and no longer publishes images; use preset 'rustfs' \
     or 'garage' (both S3-compatible)";

/// Why `preset` was removed and what to use instead, for presets berth once
/// had.
#[must_use]
pub(in crate::config) fn dropped_preset(preset: &str) -> Option<&'static str> {
    match preset {
        "minio" => Some(MINIO_REMOVED),
        _ => None,
    }
}
