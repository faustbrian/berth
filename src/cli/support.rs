//! cli support module.
//!
//! Contains cli support logic used by Berth command workflows.

mod app_runtime_context;
mod app_services;
mod artisan_command;
mod build_open_summary_json;
mod default_env_path;
mod ensure_sql_service;
mod env_output_path;
mod filter_services;
mod find_sensitive_env_values;
mod for_each_service;
mod matches_filter;
mod normalize_path;
mod open_in_browser;
mod open_summary_data;
mod persist_runtime_ports;
mod ports;
mod print_open_summary;
mod probe_http_status;
mod profile_names;
mod project_runtime_context;
mod random_ports_preflight;
mod recreate_non_app_service;
mod recreate_service;
mod resolve_profile_targets;
mod resolve_tty;
mod resolve_up_services;
mod resolve_workspace_path;
mod run_doctor;
mod run_random_ports_flow;
mod run_service_command;
mod runtime_app_env;
mod scrub_env_file;
mod select_up_targets;
mod selected_runtime_services;
mod selected_services;
mod serve_health;
mod service_labels;
mod service_scope;
mod start_service;
mod tail_access_logs;
mod workspace_root;
mod workspace_with_project_deps;

pub(crate) use app_runtime_context::{
    resolve_app_runtime_context, resolve_app_runtime_context_with_workspace_root,
};
pub(crate) use app_services::app_services;
pub(crate) use artisan_command::{build_artisan_command, build_artisan_subcommand};
pub(crate) use build_open_summary_json::build_open_summary_json;
pub(crate) use default_env_path::default_env_path;
pub(crate) use ensure_sql_service::ensure_sql_service;
pub(crate) use env_output_path::{ENV_WRITE_REMOVED, env_output_path};
pub(crate) use filter_services::filter_services;
pub(crate) use for_each_service::{
    for_each_service, run_selected_services, run_services_with_app_last,
};
pub(crate) use matches_filter::matches_filter;
pub(crate) use open_in_browser::open_in_browser;
#[cfg(test)]
pub(crate) use open_in_browser::with_open_command;
pub(crate) use open_summary_data::open_summary_data;
pub(crate) use persist_runtime_ports::persist_random_runtime_ports;
pub(crate) use ports::{
    apply_runtime_binding, collect_service_host_ports, insert_service_host_ports,
    is_port_available_strict, random_unused_port, remap_random_ports, service_host_ports,
};
#[cfg(test)]
pub(crate) use ports::{is_port_available, random_free_port};
pub(crate) use print_open_summary::print_open_summary;
pub(crate) use probe_http_status::probe_http_status;
pub(crate) use probe_http_status::run_curl_command_with_body;
#[cfg(test)]
pub(crate) use probe_http_status::with_curl_command;
pub(crate) use profile_names::profile_names;
pub(crate) use project_runtime_context::resolve_project_runtime_context;
pub(crate) use random_ports_preflight::{PreparedRandomPorts, prepare_random_ports};
pub(crate) use recreate_non_app_service::recreate_non_app_service;
pub(crate) use recreate_service::recreate_service;
pub(crate) use resolve_profile_targets::resolve_profile_targets;
pub(crate) use resolve_tty::effective_tty;
pub(crate) use resolve_up_services::resolve_up_services;
pub(crate) use resolve_workspace_path::resolve_workspace_path;
pub(crate) use run_doctor::{
    DoctorCheck, RunDoctorOptions, run_doctor, start_recording as start_doctor_recording,
    take_recording as take_doctor_recording,
};
pub(crate) use run_random_ports_flow::{RandomPortsPersistenceOptions, run_random_ports_flow};
pub(crate) use run_service_command::{run_service_command_with_tty, run_service_commands};
pub(crate) use runtime_app_env::runtime_app_env;
pub(crate) use scrub_env_file::scrub_env_file;
pub(crate) use select_up_targets::select_up_targets;
pub(crate) use selected_runtime_services::selected_runtime_services;
pub(crate) use selected_services::{selected_services, selected_services_with_filters};
pub(crate) use serve_health::{build_health_url, health_status_accepted};
pub(crate) use service_labels::{driver_name, kind_name};
pub(crate) use service_scope::ServiceScope;
pub(crate) use start_service::{ServiceStartContext, is_health_checked, start_service};
pub(crate) use tail_access_logs::tail_access_logs;
pub(crate) use workspace_root::workspace_root;
pub(crate) use workspace_with_project_deps::{
    WorkspaceWithProjectDepsOptions, workspace_with_project_deps,
};
