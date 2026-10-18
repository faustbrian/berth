//! Shared servers: one server per compatible engine and image, holding a
//! logical tenant (database and role) per instance and runtime environment.

mod acl_seed_script;
mod assign;
mod engine;
mod gc;
mod lifecycle;
mod listing;
mod mysql;
mod naming;
mod postgres;
mod redis;
mod server;
mod server_network;

pub(crate) use assign::assign_shared_target;
pub(crate) use gc::collect_servers;
pub(crate) use lifecycle::{
    detach, drop_instance_tenants, drop_tenant, refuse_server_operation, status, up,
};
