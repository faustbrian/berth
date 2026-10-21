//! Shared servers: one server per compatible engine and image, holding a
//! logical tenant (database and role) per instance and runtime environment.

mod acl_seed_script;
mod assign;
mod best_effort;
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
mod tenant_query;
mod tenant_record;

pub(crate) use assign::assign_shared_target;
pub(crate) use best_effort::best_effort;
pub(crate) use gc::collect_servers;
pub(crate) use lifecycle::{
    detach, drop_instance_tenants, drop_tenant, purge_tenant, refuse_server_operation, status, up,
};
pub(crate) use tenant_query::tenant_query;
pub(crate) use tenant_record::mark_every_server;
