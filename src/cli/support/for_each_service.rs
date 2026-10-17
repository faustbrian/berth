//! cli support for each service module.
//!
//! Contains cli support for each service logic used by Berth command workflows.

use anyhow::Result;
use rayon::prelude::*;

use crate::config;

use super::selected_services::selected_services;

pub(crate) fn for_each_service(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    driver: Option<config::Driver>,
    parallel: usize,
    f: impl Fn(&config::ServiceConfig) -> Result<()> + Sync + Send,
) -> Result<()> {
    crate::parallel::validate_parallelism(parallel)?;

    let selected = selected_services(config, service, kind, driver)?;

    run_selected_services(&selected, parallel, |svc| f(*svc))
}

pub(crate) fn run_services_with_app_last<S, IsApp, Run>(
    parallel: usize,
    services: &[S],
    is_app: IsApp,
    run: Run,
) -> Result<()>
where
    S: Sync,
    IsApp: Fn(&S) -> bool + Sync,
    Run: Fn(&S) -> Result<()> + Sync + Send,
{
    crate::parallel::validate_parallelism(parallel)?;

    if parallel <= 1 {
        for service in services {
            run(service)?;
        }
        return Ok(());
    }

    let (app_services, non_app_services): (Vec<_>, Vec<_>) =
        services.iter().partition(|service| is_app(service));

    install_in_pool(parallel, || {
        non_app_services
            .par_iter()
            .try_for_each(|service| run(service))
    })?;

    for service in app_services {
        run(service)?;
    }

    Ok(())
}

fn install_in_pool<T>(parallel: usize, run: impl FnOnce() -> Result<T> + Send) -> Result<T>
where
    T: Send,
{
    rayon::ThreadPoolBuilder::new()
        .num_threads(parallel)
        .build()?
        .install(run)
}

pub(crate) fn run_selected_services<T>(
    selected: &[T],
    parallel: usize,
    run: impl Fn(&T) -> Result<()> + Sync + Send,
) -> Result<()>
where
    T: Sync,
{
    crate::parallel::validate_parallelism(parallel)?;

    if parallel <= 1 {
        for item in selected {
            run(item)?;
        }
        return Ok(());
    }

    install_in_pool(parallel, || {
        selected.par_iter().try_for_each(|item| run(item))
    })
}

#[cfg(test)]
mod tests {
    use super::run_services_with_app_last;
    use std::sync::{Arc, Mutex};

    #[test]
    fn run_services_with_app_last_rejects_zero_parallelism() {
        let services = vec!["app".to_owned(), "cache".to_owned()];
        let result =
            run_services_with_app_last(0, &services, |service| service == "app", |_| Ok(()));
        assert!(result.is_err());
    }

    #[test]
    fn run_services_with_app_last_runs_app_services_last() {
        let services = vec!["cache".to_owned(), "app".to_owned(), "worker".to_owned()];
        let run_order = Arc::new(Mutex::new(Vec::new()));

        let run = {
            let run_order = Arc::clone(&run_order);
            move |service: &String| {
                let mut run_order = run_order
                    .lock()
                    .map_err(|_| anyhow::anyhow!("failed to lock run order"))?;
                run_order.push(service.to_owned());
                Ok(())
            }
        };

        run_services_with_app_last(2, &services, |service| service == "app", run)
            .expect("run services");

        let run_order = run_order.lock().expect("run order lock").clone();
        assert!(run_order.iter().any(|service| service == "cache"));
        assert!(run_order.iter().any(|service| service == "worker"));
        assert_eq!(run_order.last(), Some(&"app".to_owned()));
    }
}
