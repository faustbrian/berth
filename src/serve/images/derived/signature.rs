//! Signature and tag derivation for serve derived images.

/// The derived image's tag, from its content signature alone: every instance
/// and test run that needs the same image shares one tag, so parallel first
/// builds converge on it and purging an instance leaves no per-instance image
/// behind.
pub(super) fn derived_image_tag(signature: &str) -> String {
    format!("berth/serve-{:016x}", fnv1a64(signature.as_bytes()))
}

/// Derives the cache signature describing derived image contents.
pub(super) fn derive_image_signature(dockerfile: &str) -> String {
    format!(
        "dockerfile-fnv1a64-v1:{:016x}",
        fnv1a64(dockerfile.as_bytes())
    )
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::derive_image_signature;

    #[test]
    fn image_signature_is_stable_for_identical_dockerfile_content() {
        let dockerfile = "FROM base\nRUN echo hello\n";
        let first = derive_image_signature(dockerfile);
        let second = derive_image_signature(dockerfile);
        assert_eq!(first, second);
    }

    #[test]
    fn image_signature_changes_when_dockerfile_content_changes() {
        let first = derive_image_signature("FROM base\nRUN echo hello\n");
        let second = derive_image_signature("FROM base\nRUN echo world\n");
        assert_ne!(first, second);
    }
}
