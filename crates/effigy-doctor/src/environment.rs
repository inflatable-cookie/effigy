use std::collections::{HashMap, HashSet};
use std::path::Path;

use effigy_core::fs_probe::PathPresenceCache;
use effigy_core::path_probe::command_available_in_path;

use effigy_manifest::config_sections::ManifestJsPackageManager;
use effigy_manifest::{LoadedCatalog, TaskManifest};

use super::task_graph;
use crate::contracts::{check_id, install_tool, remediation};
use crate::DoctorState;

fn command_head(command: &str) -> &str {
    command.split_whitespace().next().unwrap_or_default()
}

fn required_tools_for_command(command: &str) -> &'static [&'static str] {
    match command_head(command) {
        "cargo" => &["cargo", "rustc"],
        "bun" => &["bun", "node"],
        "pnpm" => &["pnpm", "node"],
        "npm" | "npx" => &["npm", "node"],
        "node" => &["node"],
        _ => &[],
    }
}

pub(super) fn check_environment_tools(
    workspace_root: &Path,
    catalogs: &[LoadedCatalog],
    preferred_js_pm: Option<ManifestJsPackageManager>,
    state: &mut DoctorState,
) {
    let mut required = HashSet::<String>::new();
    let mut availability = ToolAvailability::new();

    let has_package_json =
        collect_required_tools(workspace_root, catalogs, &preferred_js_pm, &mut required);

    if has_package_json {
        add_required_tools(&mut required, &["node"]);
        if let Some(pm) = preferred_js_pm {
            if let Some(binary) = pm.binary_name() {
                add_required_tools(&mut required, &[binary]);
            }
        }
    }

    let mut missing = required
        .iter()
        .filter(|tool| !availability.is_available(tool))
        .map(|tool| tool.as_str())
        .collect::<Vec<&str>>();
    missing.sort();

    report_missing_tools(missing, state);

    if has_package_json
        && preferred_js_pm.is_none()
        && !availability.is_available("bun")
        && !availability.is_available("pnpm")
        && !availability.is_available("npm")
    {
        state.add_check_warning(
            check_id::ENVIRONMENT_TOOLS_REQUIRED,
            "package.json detected but no JS package manager was found (bun/pnpm/npm)",
            remediation::INSTALL_JS_PM_OR_CONFIGURE,
        );
    }
}

fn collect_required_tools(
    workspace_root: &Path,
    catalogs: &[LoadedCatalog],
    _preferred_js_pm: &Option<ManifestJsPackageManager>,
    required: &mut HashSet<String>,
) -> bool {
    let mut probe = PathPresenceCache::new();

    if probe.child_exists(workspace_root, "Cargo.toml") {
        add_required_tools(required, &["cargo", "rustc"]);
    }

    let mut has_package_json = probe.child_exists(workspace_root, "package.json");
    for catalog in catalogs {
        if probe.child_exists(&catalog.catalog_root, "Cargo.toml") {
            add_required_tools(required, &["cargo", "rustc"]);
        }
        if probe.child_exists(&catalog.catalog_root, "package.json") {
            has_package_json = true;
        }
        collect_required_tools_from_manifest(&catalog.manifest, required);
    }

    has_package_json
}

struct ToolAvailability {
    cache: HashMap<String, bool>,
}

impl ToolAvailability {
    fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    fn is_available(&mut self, tool: &str) -> bool {
        if let Some(available) = self.cache.get(tool) {
            return *available;
        }
        let available = command_available_in_path(tool);
        self.cache.insert(tool.to_owned(), available);
        available
    }
}

fn add_required_tools(required: &mut HashSet<String>, tools: &[&str]) {
    required.extend(tools.iter().map(|tool| (*tool).to_owned()));
}

fn report_missing_tools(missing: Vec<&str>, state: &mut DoctorState) {
    for tool in missing {
        state.add_check_error(
            check_id::ENVIRONMENT_TOOLS_REQUIRED,
            format!("required tool `{tool}` is not available in PATH"),
            install_tool(tool),
        );
    }
}

fn collect_required_tools_from_manifest(manifest: &TaskManifest, required: &mut HashSet<String>) {
    task_graph::for_each_manifest_command(manifest, |command| {
        detect_tools_in_command(command, required)
    });
}

fn detect_tools_in_command(command: &str, required: &mut HashSet<String>) {
    add_required_tools(required, required_tools_for_command(command));
}

#[cfg(test)]
mod tooling_tests {
    use super::{command_head, required_tools_for_command};

    #[test]
    fn required_tools_by_command() {
        assert_eq!(command_head("cargo build"), "cargo");
        assert_eq!(
            required_tools_for_command("cargo build"),
            &["cargo", "rustc"]
        );
        assert_eq!(required_tools_for_command("bun x vitest"), &["bun", "node"]);
        assert_eq!(
            required_tools_for_command("pnpm exec vitest"),
            &["pnpm", "node"]
        );
        assert_eq!(
            required_tools_for_command("npx vitest run"),
            &["npm", "node"]
        );
        assert_eq!(required_tools_for_command("node script.js"), &["node"]);
        assert!(required_tools_for_command("echo hello").is_empty());
    }
}
