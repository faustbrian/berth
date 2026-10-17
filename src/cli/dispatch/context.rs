//! Shared CLI dispatch context for commonly forwarded invocation settings.

use std::path::Path;

use crate::cli::args::Cli;

pub(crate) struct CliDispatchContext<'a> {
    quiet: bool,
    no_color: bool,
    dry_run: bool,
    non_interactive: bool,
    repro: bool,
    runtime_env: Option<&'a str>,
    config_path: Option<&'a Path>,
    project_root: Option<&'a Path>,
}

impl<'a> CliDispatchContext<'a> {
    pub(crate) fn from_cli(cli: &'a Cli) -> Self {
        Self {
            quiet: cli.quiet,
            no_color: cli.no_color,
            dry_run: cli.dry_run,
            non_interactive: cli.non_interactive,
            repro: cli.repro,
            runtime_env: cli.runtime_env(),
            config_path: cli.config_path(),
            project_root: cli.project_root_path(),
        }
    }

    pub(crate) const fn quiet(&self) -> bool {
        self.quiet
    }

    pub(crate) const fn no_color(&self) -> bool {
        self.no_color
    }

    pub(crate) const fn dry_run(&self) -> bool {
        self.dry_run
    }

    pub(crate) const fn non_interactive(&self) -> bool {
        self.non_interactive
    }

    pub(crate) const fn repro(&self) -> bool {
        self.repro
    }

    pub(crate) const fn runtime_env(&self) -> Option<&'a str> {
        self.runtime_env
    }

    pub(crate) const fn config_path(&self) -> Option<&'a Path> {
        self.config_path
    }

    pub(crate) const fn project_root(&self) -> Option<&'a Path> {
        self.project_root
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::cli::args::Cli;

    use super::CliDispatchContext;

    #[test]
    fn context_exposes_cli_flags() {
        let cli = Cli::parse_from([
            "berth",
            "--quiet",
            "--dry-run",
            "--non-interactive",
            "--no-color",
            "--env",
            "integration",
            "--project-root",
            "/tmp/example",
            "status",
        ]);
        let context = CliDispatchContext::from_cli(&cli);

        assert!(context.quiet());
        assert!(context.dry_run());
        assert!(context.non_interactive());
        assert!(context.no_color());
        assert_eq!(context.runtime_env(), Some("integration"));
        assert_eq!(context.config_path(), None);
        assert_eq!(
            context.project_root().map(|path| path.to_string_lossy()),
            Some("/tmp/example".into())
        );
    }

    #[test]
    fn context_defaults() {
        let cli = Cli::parse_from(["berth", "status"]);
        let context = CliDispatchContext::from_cli(&cli);

        assert!(!context.quiet());
        assert!(!context.no_color());
        assert!(!context.dry_run());
        assert!(!context.non_interactive());
        assert!(!context.repro());
        assert_eq!(context.runtime_env(), None);
        assert_eq!(context.config_path(), None);
        assert_eq!(context.project_root(), None);
    }
}
