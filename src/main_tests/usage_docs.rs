use super::Cli;
use clap::CommandFactory;
use std::collections::BTreeSet;

#[test]
fn usage_docs_cover_all_top_level_commands() {
    let documented = documented_top_level_commands(include_str!("../../USAGE.md"));
    let actual = cli_top_level_commands();

    let missing: Vec<_> = actual.difference(&documented).cloned().collect();
    let stale: Vec<_> = documented.difference(&actual).cloned().collect();

    assert!(
        missing.is_empty(),
        "USAGE.md is missing top-level commands: {}",
        missing.join(", ")
    );
    assert!(
        stale.is_empty(),
        "USAGE.md documents unknown top-level commands: {}",
        stale.join(", ")
    );
}

fn cli_top_level_commands() -> BTreeSet<String> {
    Cli::command()
        .get_subcommands()
        .map(|subcommand| subcommand.get_name().to_owned())
        .filter(|name| name != "help")
        .collect()
}

fn documented_top_level_commands(usage: &str) -> BTreeSet<String> {
    usage.lines().filter_map(parse_command_heading).collect()
}

fn parse_command_heading(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed.starts_with("### `berth ") {
        return None;
    }
    let command = trimmed
        .strip_prefix("### `berth ")?
        .split('`')
        .next()?
        .split_whitespace()
        .next()?;
    Some(command.to_owned())
}
