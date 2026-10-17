//! The instance requested on the command line, bound to its project root.
//!
//! `--instance` (or `BERTH_INSTANCE`) names the environment of the project the
//! command was run in. Commands reload that project's config several times in
//! one process (test runtimes, post-restore hooks, swarm injection); binding
//! the request to the canonical project root makes every reload of that
//! project use it, while other projects (swarm dependencies) keep their own
//! path-derived instances.

use std::path::{Path, PathBuf};

#[cfg(not(test))]
static REQUESTED: std::sync::Mutex<Option<(PathBuf, String)>> = std::sync::Mutex::new(None);

#[cfg(test)]
thread_local! {
    static REQUESTED: std::cell::RefCell<Option<(PathBuf, String)>> =
        const { std::cell::RefCell::new(None) };
}

/// Records that `instance` was requested for the project at `canonical_root`.
pub(crate) fn bind_requested_instance(canonical_root: &Path, instance: &str) {
    let binding = Some((canonical_root.to_path_buf(), instance.to_owned()));
    #[cfg(not(test))]
    {
        *REQUESTED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = binding;
    }
    #[cfg(test)]
    REQUESTED.with(|requested| *requested.borrow_mut() = binding);
}

/// Returns the instance requested for the project at `canonical_root`, if any.
pub(crate) fn requested_instance_for(canonical_root: &Path) -> Option<String> {
    let lookup = |binding: &Option<(PathBuf, String)>| {
        binding
            .as_ref()
            .filter(|(root, _)| root == canonical_root)
            .map(|(_, instance)| instance.clone())
    };
    #[cfg(not(test))]
    {
        lookup(
            &REQUESTED
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }
    #[cfg(test)]
    {
        REQUESTED.with(|requested| lookup(&requested.borrow()))
    }
}
