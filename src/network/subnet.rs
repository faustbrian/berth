//! Deterministic `/24` subnet candidates from `10.128.0.0/9`.
//!
//! Docker's default address pools are exhausted after a few dozen bridge
//! networks (8.x FI-59 failed at 27), which 30 parallel instances would hit.
//! Berth therefore assigns explicit subnets from a range Docker does not use
//! by default, starting at a hash of the network identity and probing forward
//! when a subnet is already taken.

use sha2::{Digest, Sha256};

/// Number of `/24` networks in `10.128.0.0/9`.
const SLOTS: u32 = 1 << 15;

/// Returns the `attempt`-th candidate subnet for a network.
#[must_use]
pub(crate) fn candidate_subnet(installation: &str, network: &str, attempt: u32) -> String {
    let digest = Sha256::digest(format!("berth-network-v1\0{installation}\0{network}"));
    let start = u32::from(digest.first().copied().unwrap_or(0) & 0x7f) << 8
        | u32::from(digest.get(1).copied().unwrap_or(0));
    let slot = (start + attempt) % SLOTS;
    format!("10.{}.{}.0/24", 128 + (slot >> 8), slot & 0xff)
}

#[cfg(test)]
mod tests {
    use super::candidate_subnet;

    #[test]
    fn candidates_are_deterministic_distinct_and_in_range() {
        let first = candidate_subnet("install", "berth_api_a-1", 0);
        assert_eq!(first, candidate_subnet("install", "berth_api_a-1", 0));
        assert_ne!(first, candidate_subnet("install", "berth_api_a-1", 1));
        for attempt in 0..300 {
            let subnet = candidate_subnet("install", "berth_api_a-1", attempt);
            let second_octet: u32 = subnet
                .split('.')
                .nth(1)
                .and_then(|octet| octet.parse().ok())
                .expect("octet");
            assert!((128..=255).contains(&second_octet), "{subnet}");
        }
    }
}
