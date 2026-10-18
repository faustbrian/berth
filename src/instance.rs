//! Berth instances: isolated environments of one project, one per checkout.

mod catalog;
mod free_test_slots;
mod identity;
mod lease;
mod owner;
mod reap;
mod requested;
mod root_record;

pub(crate) use catalog::{InstanceRecord, list_instances};
pub(crate) use identity::{InstanceIdentity, resolve_instance};
pub(crate) use lease::{InUse, hold_in_use, now_secs, parse_ttl, renew_lease};
mod ttl_change;
pub(crate) use owner::owner_process;
pub(crate) use reap::reap;
pub(crate) use requested::{bind_requested_instance, requested_instance_for};
pub(crate) use root_record::record_root;
pub(crate) use ttl_change::TtlChange;
