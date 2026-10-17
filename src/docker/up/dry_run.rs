//! docker up dry run module.
//!
//! Contains docker up dry run logic used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::super::{PullPolicy, print_docker_command};
use super::args_builder;

pub(super) fn describe(
    service: &ServiceConfig,
    pull: PullPolicy,
    recreate: bool,
    container_name: &str,
) -> Result<()> {
    if recreate {
        print_docker_command(&["rm".to_owned(), "-f".to_owned(), container_name.to_owned()]);
    }

    match pull {
        PullPolicy::Always => print_docker_command(&["pull".to_owned(), service.image.clone()]),
        PullPolicy::Missing => {
            print_docker_command(&[
                "image".to_owned(),
                "inspect".to_owned(),
                service.image.clone(),
            ]);
            print_docker_command(&["pull".to_owned(), service.image.clone()]);
        }
        PullPolicy::Never => {}
    }

    let run_args = args_builder::build_run_args(service, container_name)?;
    print_docker_command(&run_args);
    output::event(
        &service.name,
        LogLevel::Info,
        &format!("[dry-run] Ensure container {container_name} is running"),
        Persistence::Transient,
    );
    Ok(())
}
