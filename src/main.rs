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

use anyhow::Result;
use clap::Parser;

mod cli;

use cli::args::Cli;
mod config;
mod daemon;
mod database;
mod dependency_order;
mod display;
mod docker;
mod env;
mod javascript;
mod output;
mod parallel;
mod serve;
mod share;
mod swarm;

fn main() -> Result<()> {
    let cli = Cli::parse();
    cli::dispatch::run(cli)
}

#[cfg(test)]
#[path = "main_tests/mod.rs"]
mod tests;
