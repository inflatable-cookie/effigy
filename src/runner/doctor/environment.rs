use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::fs_probe::PathPresenceCache;
use crate::path_probe::command_available_in_path;

use super::super::manifest::config_sections::ManifestJsPackageManager;
use super::super::tooling::{js_package_manager_binary, required_tools_for_command};
use super::super::TaskManifest;
use super::contracts::{check_id, install_tool, remediation};
use super::report::DoctorState;
use super::task_graph;
use effigy_manifest::LoadedCatalog;

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
            if let Some(binary) = js_package_manager_binary(pm) {
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
