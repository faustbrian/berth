//! Berth instances: isolated environments of one project, one per checkout.

mod identity;
mod requested;

pub(crate) use identity::{InstanceIdentity, resolve_instance};
pub(crate) use requested::{bind_requested_instance, requested_instance_for};
