//! Berth instances: isolated environments of one project, one per checkout.

mod identity;
mod requested;

pub(crate) use identity::{InstanceIdentity, resolve_instance};
#[cfg(test)]
pub(crate) use identity::{derive_instance_id, validate_instance_id};
pub(crate) use requested::{bind_requested_instance, requested_instance_for};
