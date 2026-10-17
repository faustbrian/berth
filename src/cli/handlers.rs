//! cli handlers module.
//!
//! Contains cli handlers logic used by Berth command workflows.

mod about_cmd;
mod app_create_cmd;
mod artisan_cmd;
mod config_cmd;
mod daemon_cmd;
mod docker_ops;
mod doctor_cmd;
mod down_cmd;
mod dump_cmd;
pub(crate) mod env_cmd;
mod env_scrub_cmd;
mod exec_cmd;
mod gc_cmd;
mod health_cmd;
mod instances_cmd;
mod list_cmd;
mod lock_cmd;
mod log;
mod logs_cmd;
mod open_cmd;
mod package_manager_cmd;
mod preset_cmd;
mod profile_cmd;
mod pull_cmd;
mod recreate_cmd;
mod relabel_cmd;
mod restart_cmd;
mod restore_cmd;
mod rm_cmd;
mod serialize;
mod serve_cmd;
mod service_scope;
mod share_cmd;
mod start_cmd;
mod status_cmd;
mod stop_cmd;
mod swarm_cmd;
mod task_cmd;
mod up_cmd;
mod update_cmd;
mod url_cmd;

pub(crate) use about_cmd::handle_about;
pub(crate) use app_create_cmd::{HandleAppCreateOptions, handle_app_create};
pub(crate) use artisan_cmd::{
    HandleArtisanOptions, handle_artisan, set_testing_runtime_pool_size_override,
};
pub(crate) use config_cmd::{handle_config, handle_config_migrate, handle_config_validate};
pub(crate) use daemon_cmd::handle_daemon;
pub(crate) use docker_ops::{
    HandleAttachOptions, HandleCpOptions, HandleEventsOptions, HandleInspectOptions,
    HandlePortOptions, HandlePruneOptions, handle_attach, handle_cp, handle_events, handle_inspect,
    handle_kill, handle_pause, handle_port, handle_prune, handle_stats, handle_top, handle_unpause,
    handle_wait,
};
pub(crate) use doctor_cmd::{HandleDoctorOptions, handle_doctor};
pub(crate) use down_cmd::{HandleDownOptions, handle_down};
pub(crate) use dump_cmd::{HandleDumpOptions, handle_dump};
pub(crate) use env_cmd::{HandleEnvOptions, handle_env};
pub(crate) use env_scrub_cmd::handle_env_scrub;
pub(crate) use exec_cmd::{HandleExecOptions, handle_exec};
pub(crate) use gc_cmd::handle_gc;
pub(crate) use health_cmd::{HandleHealthOptions, handle_health};
pub(crate) use instances_cmd::handle_list_instances;
pub(crate) use list_cmd::handle_list;
pub(crate) use lock_cmd::handle_lock;
pub(crate) use logs_cmd::{HandleLogsOptions, handle_logs};
pub(crate) use open_cmd::{HandleOpenOptions, handle_open};
pub(crate) use package_manager_cmd::{
    HandlePackageManagerCommandOptions, handle_package_manager_command,
};
pub(crate) use preset_cmd::{handle_preset_list, handle_preset_show};
pub(crate) use profile_cmd::{handle_profile_list, handle_profile_show};
pub(crate) use pull_cmd::handle_pull;
pub(crate) use recreate_cmd::{HandleRecreateOptions, handle_recreate};
pub(crate) use relabel_cmd::{HandleRelabelOptions, handle_relabel};
pub(crate) use restart_cmd::{HandleRestartOptions, handle_restart};
pub(crate) use restore_cmd::{HandleRestoreOptions, handle_restore};
pub(crate) use rm_cmd::{HandleRmOptions, handle_rm};
pub(crate) use serve_cmd::{HandleServeOptions, handle_serve};
pub(crate) use share_cmd::{
    HandleShareStartOptions, HandleShareStopOptions, handle_share_start, handle_share_status,
    handle_share_stop,
};
pub(crate) use start_cmd::{HandleStartOptions, handle_start};
pub(crate) use status_cmd::handle_status;
pub(crate) use stop_cmd::handle_stop;
pub(crate) use swarm_cmd::{HandleSwarmOptions, handle_swarm};
pub(crate) use task_cmd::{
    HandleTaskDepsWorkflowOptions, TaskDependencyAction, handle_task_deps_workflow,
};
pub(crate) use up_cmd::{HandleUpOptions, UpReportOptions, failed_report, handle_up, ready_report};
pub(crate) use update_cmd::{HandleUpdateOptions, handle_update};
pub(crate) use url_cmd::handle_url;
