//! A service's `cpus` and `memory` caps, checked as the engine reads them.

use anyhow::Result;

use crate::config::types::ResourceLimits;

/// Checks `cpus` (a positive number) and `memory` (a positive whole number
/// of bytes, or of `k`, `m` or `g`: Docker's and Podman's `--memory` units)
/// before any container is made with them.
pub(super) fn resource_limits(cpus: Option<f64>, memory: Option<&str>) -> Result<ResourceLimits> {
    let cpus = cpus
        .map(|cpus| {
            // Docker and Podman refuse less than a hundredth of a CPU.
            if cpus.is_finite() && cpus >= 0.01 {
                Ok(cpus.to_string())
            } else {
                anyhow::bail!(
                    "`cpus: {cpus}` must be a number of CPUs of at least 0.01, such as 2 or 0.5"
                )
            }
        })
        .transpose()?;
    let memory = memory
        .map(|memory| {
            let digits = memory.trim_end_matches(['b', 'k', 'm', 'g', 'B', 'K', 'M', 'G']);
            let unit_len = memory.len() - digits.len();
            if unit_len > 1
                || digits.is_empty()
                || !digits.bytes().all(|byte| byte.is_ascii_digit())
                || digits.bytes().all(|byte| byte == b'0')
            {
                anyhow::bail!(
                    "`memory: {memory}` must be a positive size: a whole number with `k`, `m` \
                     or `g` (`512m`, `2g`), or bytes"
                );
            }
            // Docker and Podman refuse less than 6 MiB.
            let number: u128 = digits
                .parse()
                .map_err(|_| anyhow::anyhow!("`memory: {memory}` is too large"))?;
            let unit: u128 = match memory
                .get(digits.len()..)
                .map(str::to_ascii_lowercase)
                .as_deref()
            {
                Some("k") => 1 << 10,
                Some("m") => 1 << 20,
                Some("g") => 1 << 30,
                _ => 1,
            };
            // The engines take the size as a signed 64-bit count of bytes.
            let bytes = number
                .checked_mul(unit)
                .filter(|bytes| *bytes <= u128::from(i64::MAX.unsigned_abs()))
                .ok_or_else(|| anyhow::anyhow!("`memory: {memory}` is too large"))?;
            if bytes < 6 << 20 {
                anyhow::bail!("`memory: {memory}` must be at least 6m (the engines' minimum)");
            }
            Ok(memory.to_ascii_lowercase())
        })
        .transpose()?;
    Ok(ResourceLimits { cpus, memory })
}

#[cfg(test)]
mod tests {
    use super::resource_limits;

    #[test]
    fn caps_are_checked_as_the_engine_reads_them() {
        let limits = resource_limits(Some(2.0), Some("512M")).expect("valid");
        assert_eq!(
            (limits.cpus.as_deref(), limits.memory.as_deref()),
            (Some("2"), Some("512m"))
        );
        assert_eq!(
            resource_limits(Some(0.5), None)
                .expect("valid")
                .cpus
                .as_deref(),
            Some("0.5")
        );
        assert!(resource_limits(Some(0.0), None).is_err());
        assert!(resource_limits(Some(f64::NAN), None).is_err());
        assert!(resource_limits(Some(0.001), None).is_err());
        assert!(resource_limits(None, Some("5m")).is_err());
        assert!(resource_limits(None, Some("6291456")).is_ok());
        for huge in ["99999999999999999999999999999999999999999g", "99999999999g"] {
            let error = resource_limits(None, Some(huge))
                .expect_err(huge)
                .to_string();
            assert!(error.contains("too large"), "{error}");
        }
        for memory in ["", "m", "0m", "1.5g", "512mb", "-1g", "lots"] {
            assert!(resource_limits(None, Some(memory)).is_err(), "{memory}");
        }
    }
}
