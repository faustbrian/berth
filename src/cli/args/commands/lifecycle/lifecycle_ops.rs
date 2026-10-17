//! cli args commands lifecycle lifecycle ops module.
//!
//! Contains cli args commands lifecycle lifecycle ops logic used by Berth command workflows.

use clap::Args;

use crate::cli::args::default_parallelism;
use crate::config;

#[derive(Args)]
pub(crate) struct DownArgs {
    #[arg(long)]
    pub(crate) service: Vec<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Down a named service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind"])]
    pub(crate) profile: Option<String>,
    /// Skip stopping workspace swarm dependencies
    #[arg(long, default_value_t = false)]
    pub(crate) no_deps: bool,
    /// Allow downing shared workspace dependencies
    #[arg(long, short, default_value_t = false, conflicts_with = "no_deps")]
    pub(crate) force: bool,
    /// Timeout in seconds for stop before remove
    #[arg(long, default_value_t = 30)]
    pub(crate) timeout: u64,
    #[arg(long, default_value_t = default_parallelism())]
    pub(crate) parallel: usize,
}

impl DownArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.first().map(String::as_str)
    }

    pub(crate) fn services(&self) -> &[String] {
        &self.service
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }

    pub(crate) fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}

#[derive(Args)]
pub(crate) struct StopArgs {
    #[arg(long)]
    pub(crate) service: Vec<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Stop a named service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind"])]
    pub(crate) profile: Option<String>,
    /// Timeout in seconds for docker stop
    #[arg(long, default_value_t = 30)]
    pub(crate) timeout: u64,
    #[arg(long, default_value_t = default_parallelism())]
    pub(crate) parallel: usize,
}

impl StopArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.first().map(String::as_str)
    }

    pub(crate) fn services(&self) -> &[String] {
        &self.service
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }

    pub(crate) fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}

#[derive(Args)]
pub(crate) struct RmArgs {
    #[arg(long)]
    pub(crate) service: Vec<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Remove a named service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind"])]
    pub(crate) profile: Option<String>,
    #[arg(long, short, default_value_t = false)]
    pub(crate) force: bool,
    #[arg(long, default_value_t = default_parallelism())]
    pub(crate) parallel: usize,
}

impl RmArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.first().map(String::as_str)
    }

    pub(crate) fn services(&self) -> &[String] {
        &self.service
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }

    pub(crate) fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}

#[derive(Args)]
pub(crate) struct RecreateArgs {
    #[arg(long)]
    pub(crate) service: Vec<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Recreate a named service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind"])]
    pub(crate) profile: Option<String>,
    /// Wait for service(s) to accept connections after recreating
    /// (enabled by default)
    #[arg(long, default_value_t = false)]
    pub(crate) wait: bool,
    /// Skip health waits during recreate
    #[arg(long, default_value_t = false, conflicts_with = "wait")]
    pub(crate) no_wait: bool,
    /// Timeout in seconds for --wait
    #[arg(long, default_value_t = 30)]
    pub(crate) wait_timeout: u64,
    /// Publish all exposed ports to random host ports after start
    #[arg(long, short = 'P', default_value_t = false)]
    pub(crate) publish_all: bool,
    /// Skip recreating workspace swarm dependencies
    #[arg(long, default_value_t = false)]
    pub(crate) no_deps: bool,
    /// Write inferred service vars to local `.env`
    #[arg(long, default_value_t = false)]
    pub(crate) env_output: bool,
    #[arg(long, default_value_t = default_parallelism())]
    pub(crate) parallel: usize,
}

impl RecreateArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.first().map(String::as_str)
    }

    pub(crate) fn services(&self) -> &[String] {
        &self.service
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }

    pub(crate) fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }

    pub(crate) const fn should_wait(&self) -> bool {
        !self.no_wait
    }
}

#[derive(Args)]
pub(crate) struct RestartArgs {
    #[arg(long)]
    pub(crate) service: Vec<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Restart a named service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind"])]
    pub(crate) profile: Option<String>,
    #[arg(long, default_value_t = false)]
    pub(crate) wait: bool,
    #[arg(long, default_value_t = 30)]
    pub(crate) wait_timeout: u64,
    #[arg(long, default_value_t = default_parallelism())]
    pub(crate) parallel: usize,
}

impl RestartArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.first().map(String::as_str)
    }

    pub(crate) fn services(&self) -> &[String] {
        &self.service
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }

    pub(crate) fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}

#[derive(Args)]
pub(crate) struct RelabelArgs {
    #[arg(long)]
    pub(crate) service: Vec<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Relabel a named service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind"])]
    pub(crate) profile: Option<String>,
    /// Wait for service(s) to accept connections after relabeling
    #[arg(long, default_value_t = false)]
    pub(crate) wait: bool,
    /// Timeout in seconds for --wait
    #[arg(long, default_value_t = 30)]
    pub(crate) wait_timeout: u64,
    #[arg(long, default_value_t = default_parallelism())]
    pub(crate) parallel: usize,
}

impl RelabelArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.first().map(String::as_str)
    }

    pub(crate) fn services(&self) -> &[String] {
        &self.service
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }

    pub(crate) fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use crate::cli::args::commands::lifecycle::lifecycle_ops::{
        DownArgs, RecreateArgs, RelabelArgs, RestartArgs, RmArgs, StopArgs,
    };
    use crate::cli::args::default_parallelism;
    use crate::config::Kind;

    #[test]
    fn getters_return_service_filter() {
        let args = DownArgs {
            service: vec!["api".to_owned()],
            kind: Some(Kind::App),
            profile: None,
            no_deps: false,
            force: false,
            timeout: 30,
            parallel: 1,
        };

        assert_eq!(args.service(), Some("api"));
        assert_eq!(args.kind(), Some(Kind::App));
        assert!(!args.no_deps);
    }

    #[test]
    fn helper_defaults_are_present() {
        let args = StopArgs {
            service: Vec::new(),
            kind: None,
            profile: None,
            timeout: 30,
            parallel: default_parallelism(),
        };
        assert_eq!(args.service(), None);
        assert_eq!(args.parallel, default_parallelism());
        assert_eq!(args.kind(), None);
    }

    #[test]
    fn other_lifecycle_getters_expose_parsed_fields() {
        let args = RmArgs {
            service: Vec::new(),
            kind: Some(Kind::Database),
            profile: None,
            force: true,
            parallel: 2,
        };
        assert_eq!(args.kind(), Some(Kind::Database));
        assert!(args.force);

        let recreate = RecreateArgs {
            service: vec!["db".to_owned()],
            kind: Some(Kind::Database),
            profile: None,
            wait: true,
            no_wait: false,
            wait_timeout: 5,
            publish_all: false,
            no_deps: false,
            env_output: true,
            parallel: 1,
        };
        assert_eq!(recreate.service(), Some("db"));
        assert!(recreate.should_wait());
        assert_eq!(recreate.wait_timeout, 5);

        let restart = RestartArgs {
            service: vec!["redis".to_owned()],
            kind: Some(Kind::Cache),
            profile: None,
            wait: false,
            wait_timeout: 10,
            parallel: 4,
        };
        assert_eq!(restart.service(), Some("redis"));

        let relabel = RelabelArgs {
            service: Vec::new(),
            kind: None,
            profile: None,
            wait: false,
            wait_timeout: 1,
            parallel: 4,
        };
        assert_eq!(relabel.kind(), None);
    }
}
