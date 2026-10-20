//! Data service container manager CLI.
//!
//! Berth is a command-line tool for managing local data services with Docker.

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
mod error_hint;
mod followup;
mod gateway;
mod home;
mod instance;
mod javascript;
mod network;
mod output;
mod parallel;
mod serve;
mod share;
mod shared;
mod swarm;
#[cfg(test)]
mod test_support;
mod wait;

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
            let message = docker::redact_known_secrets(&format!("{error:#}"));
            eprintln!("Error: {message}");
            if let Some(hint) = error_hint::hint(&message) {
                eprintln!("Hint: {hint}");
            }
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
