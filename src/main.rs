//! Data service container manager CLI.
//!
//! Berth is a command-line tool for managing local data services with Docker.

#![allow(clippy::print_stdout)] // CLI tool needs to print to stdout
#![allow(clippy::clone_on_ref_ptr)] // Arc clones are explicit at call sites for clarity
#![allow(clippy::fn_params_excessive_bools)] // CLI options are represented directly as flags
#![allow(clippy::items_after_statements)] // Local helper functions keep related logic together
#![allow(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(
    clippy::mod_module_files,
    clippy::wildcard_enum_match_arm,
    clippy::verbose_file_reads,
    clippy::unseparated_literal_suffix,
    clippy::unreachable,
    clippy::string_slice,
    clippy::shadow_unrelated,
    clippy::shadow_reuse,
    clippy::pattern_type_mismatch,
    clippy::indexing_slicing,
    clippy::empty_structs_with_brackets,
    clippy::as_conversions,
    clippy::print_stderr
)]
#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::if_then_some_else_none,
        clippy::panic,
        clippy::str_to_string,
        clippy::unwrap_in_result,
        clippy::unwrap_used,
    )
)]

use clap::Parser;
use std::process::ExitCode;

mod cli;

use cli::args::Cli;
mod config;
mod daemon;
mod database;
mod dependency_order;
mod display;
mod docker;
mod env;
mod gateway;
mod home;
mod instance;
mod javascript;
mod network;
mod output;
mod parallel;
mod serve;
mod share;
mod swarm;

/// Exit code for failures of berth itself (as opposed to a command berth ran),
/// following the Docker CLI convention so agents can tell them apart.
const BERTH_FAILURE_EXIT_CODE: u8 = 125;

fn main() -> ExitCode {
    // Usage errors are berth failures too (clap would exit 2, which collides
    // with real test-runner exit codes); help and version still exit 0.
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            drop(error.print());
            return if error.use_stderr() {
                ExitCode::from(BERTH_FAILURE_EXIT_CODE)
            } else {
                ExitCode::SUCCESS
            };
        }
    };
    match cli::dispatch::run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!(
                "Error: {}",
                docker::redact_known_secrets(&format!("{error:#}"))
            );
            let child_code = error
                .chain()
                .find_map(|cause| cause.downcast_ref::<docker::ChildExitError>())
                .map(docker::ChildExitError::code);
            ExitCode::from(child_code.unwrap_or(BERTH_FAILURE_EXIT_CODE))
        }
    }
}

#[cfg(test)]
#[path = "main_tests/mod.rs"]
mod tests;
