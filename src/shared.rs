//! Shared servers: one server per compatible engine and image, holding a
//! logical tenant (database and role) per instance and runtime environment.

mod assign;
mod engine;
mod lifecycle;
mod listing;
mod mysql;
mod naming;
mod postgres;
mod server;

pub(crate) use assign::assign_shared_target;
pub(crate) use lifecycle::{
    detach, drop_instance_tenants, drop_tenant, refuse_server_operation, status, up,
};
