//! The optional cap `--wait-timeout` puts on a command's waits.
//!
//! berth has no deadlines of its own (how long a start takes depends on how
//! loaded the machine is), so a cap exists only when a caller sets one.

mod cap;

pub(crate) use cap::{Deadline, cap_secs, set_cap};
