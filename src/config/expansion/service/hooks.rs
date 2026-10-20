//! Service hook expansion from raw config to normalized types.

use anyhow::{Result, anyhow};

use super::super::super::{HookOnError, HookRun, ServiceHook};
use crate::config::raw::{RawHookRun, RawServiceHook};

pub(super) fn expand_hooks(raw_hooks: Vec<RawServiceHook>) -> Result<Vec<ServiceHook>> {
    raw_hooks
        .into_iter()
        .map(|raw| {
            let name = raw
                .name
                .ok_or_else(|| anyhow!("a service hook has no `name`; give each hook a `name`"))?;
            let phase = raw.phase.ok_or_else(|| {
                anyhow!(
                    "service hook '{name}' has no `phase`; set `phase:` to post_up, pre_down or \
                     post_down"
                )
            })?;
            let run = raw
                .run
                .ok_or_else(|| {
                    anyhow!(
                        "service hook '{name}' has no `run`; set `run:` to `{{type: exec, argv: \
                         [...]}}` or `{{type: script, path: <file>}}`"
                    )
                })
                .map(expand_hook_run)?;

            Ok(ServiceHook {
                name,
                phase,
                run,
                on_error: raw.on_error.unwrap_or(HookOnError::Fail),
                timeout_sec: raw.timeout_sec,
            })
        })
        .collect()
}

fn expand_hook_run(raw: RawHookRun) -> HookRun {
    match raw {
        RawHookRun::Exec { argv } => HookRun::Exec { argv },
        RawHookRun::Script { path } => HookRun::Script { path },
    }
}
