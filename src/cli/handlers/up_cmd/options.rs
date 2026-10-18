//! up command option normalization helpers.

use anyhow::Result;

pub(super) struct ExecutionFlags {
    pub(super) use_wait: bool,
    pub(super) use_publish_all: bool,
}

pub(super) fn validate_repro_flags(env_output: bool) -> Result<()> {
    if env_output {
        anyhow::bail!("--repro cannot be combined with --env-output");
    }
    Ok(())
}

pub(super) fn resolve_execution_flags(
    wait: bool,
    no_wait: bool,
    publish_all: bool,
    no_publish_all: bool,
) -> ExecutionFlags {
    ExecutionFlags {
        use_wait: if no_wait {
            false
        } else {
            wait || default_wait_enabled()
        },
        use_publish_all: if no_publish_all {
            false
        } else {
            publish_all || default_publish_all_enabled()
        },
    }
}

/// Returns the default value for publish all enabled. Off in berth:
/// services are reached inside instance networks and through gateway
/// routes, so nothing needs a (racy) random host port by default.
const fn default_publish_all_enabled() -> bool {
    false
}

/// Returns the default value for wait enabled.
const fn default_wait_enabled() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::{
        default_publish_all_enabled, default_wait_enabled, resolve_execution_flags,
        validate_repro_flags,
    };

    #[test]
    fn validate_repro_flags_rejects_conflicting_mode() {
        assert!(validate_repro_flags(true).is_err());
        assert!(validate_repro_flags(false).is_ok());
    }

    #[test]
    fn resolve_execution_flags_prefers_explicit_no_flags() {
        let flags = resolve_execution_flags(false, false, false, false);
        assert!(flags.use_wait);
        assert!(!flags.use_publish_all, "host ports are opt-in in berth");
        assert_eq!(flags.use_wait, default_wait_enabled());
        assert_eq!(flags.use_publish_all, default_publish_all_enabled());
    }

    #[test]
    fn resolve_execution_flags_supports_negative_and_explicit_options() {
        let explicit = resolve_execution_flags(true, true, false, false);
        assert!(!explicit.use_wait);

        let force_random = resolve_execution_flags(false, false, true, false);
        assert!(force_random.use_publish_all);

        let disabled = resolve_execution_flags(false, false, true, true);
        assert!(!disabled.use_publish_all);
    }
}
