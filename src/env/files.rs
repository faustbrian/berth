//! env files module.
//!
//! Contains env files logic used by Berth command workflows.

use crate::output::{self, LogLevel, Persistence};
use anyhow::{Context, Result};
use std::path::Path;

mod mutations;
mod parse;
mod update;

pub(crate) use update::update_env;

/// Reads env lines from persisted or external state.
fn read_env_lines(env_path: &Path) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(env_path)
        .with_context(|| format!("failed to read {}", env_path.display()))?;
    Ok(content.lines().map(str::to_owned).collect())
}

/// Writes env lines to persisted or external state.
fn write_env_lines(env_path: &Path, lines: Vec<String>) -> Result<()> {
    let mut output = lines.join("\n");
    output.push('\n');

    if crate::docker::is_dry_run() {
        output::event(
            "env",
            LogLevel::Info,
            &format!("[dry-run] Write {}", env_path.display()),
            Persistence::Transient,
        );
    } else {
        std::fs::write(env_path, output)
            .with_context(|| format!("failed to write {}", env_path.display()))?;
    }

    Ok(())
}

pub(super) fn quote_env_value(value: &str) -> String {
    let escaped = value.chars().fold(String::new(), |mut acc, ch| {
        match ch {
            '\\' => acc.push_str("\\\\"),
            '"' => acc.push_str("\\\""),
            '\n' => acc.push_str("\\n"),
            '\r' => acc.push_str("\\r"),
            '\t' => acc.push_str("\\t"),
            _ => acc.push(ch),
        }
        acc
    });

    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::quote_env_value;

    #[test]
    fn quote_env_value_escapes_control_and_quote_characters() {
        assert_eq!(
            quote_env_value("line\n\"quote\"\\slash\t"),
            "\"line\\n\\\"quote\\\"\\\\slash\\t\""
        );
    }
}
