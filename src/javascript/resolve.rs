use anyhow::{Context, Result};
use std::path::Path;

use super::{
    JavaScriptRuntime, JavaScriptToolchain, PackageManager, VersionManager,
    detect_javascript_runtime, detect_node_package_manager, detect_node_version,
};

pub(crate) struct ResolveJavaScriptRuntimeOptions<'a> {
    pub(crate) configured: Option<&'a JavaScriptToolchain>,
    pub(crate) workspace_root: &'a Path,
    pub(crate) runtime: Option<JavaScriptRuntime>,
    pub(crate) package_manager: Option<PackageManager>,
    pub(crate) version_manager: Option<VersionManager>,
    pub(crate) node_version: Option<&'a str>,
    pub(crate) require_package_manager: bool,
}

#[derive(Debug)]
pub(crate) struct ResolvedJavaScriptRuntime {
    pub(crate) runtime: JavaScriptRuntime,
    pub(crate) package_manager: Option<PackageManager>,
    pub(crate) version_manager: VersionManager,
    pub(crate) node_version: Option<String>,
}

pub(crate) fn resolve_javascript_runtime(
    options: ResolveJavaScriptRuntimeOptions<'_>,
) -> Result<ResolvedJavaScriptRuntime> {
    let runtime = options
        .runtime
        .or_else(|| options.configured.and_then(|config| config.runtime))
        .or_else(|| detect_javascript_runtime(options.workspace_root))
        .unwrap_or(JavaScriptRuntime::Node);
    let package_manager = options
        .package_manager
        .or_else(|| options.configured.and_then(|config| config.package_manager))
        .or_else(|| {
            if runtime == JavaScriptRuntime::Node {
                detect_node_package_manager(options.workspace_root)
            } else {
                None
            }
        });
    let version_manager = options
        .version_manager
        .or_else(|| options.configured.and_then(|config| config.version_manager))
        .unwrap_or(VersionManager::System);
    let node_version = options
        .node_version
        .map(str::to_owned)
        .or_else(|| options.configured.and_then(|config| config.version.clone()))
        .or_else(|| detect_node_version(options.workspace_root));

    if runtime == JavaScriptRuntime::Node && options.require_package_manager {
        package_manager.context(
            "could not infer the Node package manager (no package-lock.json, pnpm-lock.yaml or yarn.lock and no `packageManager` in package.json); pass --package-manager <npm|pnpm|yarn>, or set `javascript: {package_manager: npm}` on the app service in .berth.yaml",
        )?;
    }

    if runtime == JavaScriptRuntime::Node
        && version_manager != VersionManager::System
        && node_version.is_none()
    {
        anyhow::bail!(
            "node version manager '{}' needs a Node version and found no .nvmrc, .node-version or package.json `engines.node`; set `javascript: {{version: \"22\"}}` on the app service in .berth.yaml (or add a .nvmrc)",
            version_manager.as_str()
        );
    }

    if runtime == JavaScriptRuntime::Deno && options.require_package_manager {
        anyhow::bail!(
            "configured JavaScript runtime is deno; use {} instead of Node package-manager workflows",
            crate::followup::command("deno")
        );
    }

    if runtime == JavaScriptRuntime::Bun && options.require_package_manager {
        anyhow::bail!(
            "configured JavaScript runtime is bun; use {} instead of Node package-manager workflows",
            crate::followup::command("bun")
        );
    }

    Ok(ResolvedJavaScriptRuntime {
        runtime,
        package_manager,
        version_manager,
        node_version,
    })
}

#[cfg(test)]
mod tests {
    use super::{ResolveJavaScriptRuntimeOptions, resolve_javascript_runtime};
    use crate::javascript::{
        JavaScriptRuntime, JavaScriptToolchain, PackageManager, VersionManager,
    };
    use std::fs;
    use std::path::PathBuf;

    fn temp_root(prefix: &str) -> PathBuf {
        let root = crate::test_support::temp_root()
            .join(format!("{prefix}-{}", crate::test_support::unique_stamp()));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    #[test]
    fn resolve_javascript_runtime_reads_version_from_nvmrc() {
        let root = temp_root("berth-node-runtime-nvmrc");
        fs::write(root.join(".nvmrc"), "22\n").expect("write nvmrc");

        let runtime = resolve_javascript_runtime(ResolveJavaScriptRuntimeOptions {
            configured: Some(&JavaScriptToolchain {
                runtime: Some(JavaScriptRuntime::Node),
                package_manager: Some(PackageManager::Npm),
                version_manager: Some(VersionManager::Fnm),
                version: None,
            }),
            workspace_root: &root,
            runtime: None,
            package_manager: None,
            version_manager: None,
            node_version: None,
            require_package_manager: true,
        })
        .expect("resolve runtime");

        assert_eq!(runtime.node_version.as_deref(), Some("22"));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn resolve_javascript_runtime_requires_version_for_non_system_manager() {
        let root = temp_root("berth-node-runtime-version-required");

        let error = resolve_javascript_runtime(ResolveJavaScriptRuntimeOptions {
            configured: Some(&JavaScriptToolchain {
                runtime: Some(JavaScriptRuntime::Node),
                package_manager: Some(PackageManager::Npm),
                version_manager: Some(VersionManager::Volta),
                version: None,
            }),
            workspace_root: &root,
            runtime: None,
            package_manager: None,
            version_manager: None,
            node_version: None,
            require_package_manager: true,
        })
        .expect_err("missing version should fail");

        assert!(error.to_string().contains("needs a Node version"));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn resolve_javascript_runtime_detects_deno_workspace() {
        let root = temp_root("berth-js-runtime-deno");
        fs::write(root.join("deno.json"), "{\"tasks\":{}}").expect("write deno config");

        let runtime = resolve_javascript_runtime(ResolveJavaScriptRuntimeOptions {
            configured: None,
            workspace_root: &root,
            runtime: None,
            package_manager: None,
            version_manager: None,
            node_version: None,
            require_package_manager: false,
        })
        .expect("resolve deno runtime");

        assert_eq!(runtime.runtime, JavaScriptRuntime::Deno);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn resolve_javascript_runtime_detects_bun_workspace() {
        let root = temp_root("berth-js-runtime-bun");
        fs::write(root.join("bun.lock"), "{}").expect("write bun lockfile");

        let runtime = resolve_javascript_runtime(ResolveJavaScriptRuntimeOptions {
            configured: None,
            workspace_root: &root,
            runtime: None,
            package_manager: None,
            version_manager: None,
            node_version: None,
            require_package_manager: false,
        })
        .expect("resolve bun runtime");

        assert_eq!(runtime.runtime, JavaScriptRuntime::Bun);
        fs::remove_dir_all(root).expect("cleanup");
    }
}
