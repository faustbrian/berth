//! What a command asks of an instance's lease TTL.

use std::time::Duration;

/// A lease TTL request: keep the recorded one, set a new one (`--ttl 2h`) or
/// clear it (`--ttl none`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TtlChange {
    /// Keep whatever TTL the lease has.
    Keep,
    /// Expire this long after each command.
    Set(Duration),
    /// Never expire by time.
    Clear,
}
