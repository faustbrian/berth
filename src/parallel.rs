//! Shared parallel execution helpers.
//!
//! Contains reusable validation utilities for parallelism configuration.

use anyhow::Result;

mod sibling_failure;

pub(crate) use sibling_failure::{ensure_no_sibling_failed, try_for_each_ending_siblings};

pub(crate) fn validate_parallelism(parallel: usize) -> Result<()> {
    if parallel == 0 {
        anyhow::bail!("--parallel must be >= 1");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_parallelism;

    #[test]
    fn validate_parallelism_rejects_zero() {
        assert!(validate_parallelism(0).is_err());
    }

    #[test]
    fn validate_parallelism_accepts_one_or_more() {
        assert!(validate_parallelism(1).is_ok());
        assert!(validate_parallelism(4).is_ok());
    }
}
