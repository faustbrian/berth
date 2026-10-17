//! docker logs many module.
//!
//! Contains docker logs many logic used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

use super::{LogsOptions, logs, logs_prefixed};

pub(super) fn logs_many(services: &[ServiceConfig], options: LogsOptions) -> Result<()> {
    if services.is_empty() {
        return Ok(());
    }

    if !options.follow {
        for service in services {
            if options.prefix {
                logs_prefixed(
                    service,
                    LogsOptions {
                        follow: false,
                        ..options.clone()
                    },
                )?;
            } else {
                println!("== {} ==", service.name);
                logs(
                    service,
                    LogsOptions {
                        follow: false,
                        ..options.clone()
                    },
                )?;
            }
        }
        return Ok(());
    }

    let handles: Vec<std::thread::JoinHandle<Result<()>>> = services
        .iter()
        .cloned()
        .map(|service| {
            let options = options.clone();
            std::thread::spawn(move || {
                if options.prefix {
                    logs_prefixed(&service, options)
                } else {
                    logs(&service, options)
                }
            })
        })
        .collect();

    for handle in handles {
        let result = handle
            .join()
            .map_err(|_| anyhow::anyhow!("failed joining logs thread"))?;
        result?;
    }

    Ok(())
}
