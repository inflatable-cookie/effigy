use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::super::{
    LoadedCatalog, ManifestJsPackageManager, ManifestManagedConcurrentEntry, ManifestManagedRun,
    ManifestManagedRunStep, TaskManifest,
};
use super::{DoctorFinding, DoctorSeverity};

pub(super) fn check_environment_tools(
    workspace_root: &Path,
    catalogs: &[LoadedCatalog],
    preferred_js_pm: Option<ManifestJsPackageManager>,
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    let mut required = HashSet::<&str>::new();

    if workspace_root.join("Cargo.toml").exists() {
        required.insert("cargo");
        required.insert("rustc");
    }

    let mut has_package_json = workspace_root.join("package.json").exists();
    for catalog in catalogs {
        if catalog.catalog_root.join("Cargo.toml").exists() {
            required.insert("cargo");
            required.insert("rustc");
        }
        if catalog.catalog_root.join("package.json").exists() {
            has_package_json = true;
        }
        collect_required_tools_from_manifest(&catalog.manifest, &mut required);
    }

    if has_package_json {
        required.insert("node");
        if let Some(pm) = preferred_js_pm {
            match pm {
                ManifestJsPackageManager::Bun => {
                    required.insert("bun");
                }
                ManifestJsPackageManager::Pnpm => {
                    required.insert("pnpm");
                }
                ManifestJsPackageManager::Npm => {
                    required.insert("npm");
                }
                ManifestJsPackageManager::Direct => {}
            }
        }
    }

    let mut missing = required
        .iter()
        .filter(|tool| !tool_available(tool))
        .copied()
        .collect::<Vec<&str>>();
    missing.sort();

    for tool in missing {
        super::add_finding(
            findings,
            statuses,
            DoctorFinding {
                check_id: "environment.tools.required".to_owned(),
                severity: DoctorSeverity::Error,
                evidence: format!("required tool `{tool}` is not available in PATH"),
                remediation: format!("Install `{tool}` and re-run `effigy doctor`."),
                fixable: false,
            },
        );
    }

    if has_package_json
        && preferred_js_pm.is_none()
        && !tool_available("bun")
        && !tool_available("pnpm")
        && !tool_available("npm")
    {
        super::add_finding(
            findings,
            statuses,
            DoctorFinding {
                check_id: "environment.tools.required".to_owned(),
                severity: DoctorSeverity::Warning,
                evidence: "package.json detected but no JS package manager was found (bun/pnpm/npm)"
                    .to_owned(),
                remediation: "Install one of bun/pnpm/npm or define `[package_manager].js` to match your toolchain.".to_owned(),
                fixable: false,
            },
        );
    }
}

fn collect_required_tools_from_manifest<'a>(
    manifest: &'a TaskManifest,
    required: &mut HashSet<&'a str>,
) {
    for task in manifest.tasks.values() {
        if let Some(run) = task.run.as_ref() {
            match run {
                ManifestManagedRun::Command(command) => detect_tools_in_command(command, required),
                ManifestManagedRun::Sequence(steps) => {
                    for step in steps {
                        match step {
                            ManifestManagedRunStep::Command(command) => {
                                detect_tools_in_command(command, required)
                            }
                            ManifestManagedRunStep::Step(table) => {
                                if let Some(run) = table.run.as_ref() {
                                    detect_tools_in_command(run, required);
                                }
                            }
                        }
                    }
                }
            }
        }
        collect_tools_from_entries(&task.concurrent, required);
        for profile in task.profiles.values() {
            collect_tools_from_entries(&profile.concurrent, required);
        }
    }
}

fn collect_tools_from_entries<'a>(
    entries: &'a [ManifestManagedConcurrentEntry],
    required: &mut HashSet<&'a str>,
) {
    for entry in entries {
        if let Some(run) = entry.run.as_ref() {
            detect_tools_in_command(run, required);
        }
    }
}

fn detect_tools_in_command<'a>(command: &'a str, required: &mut HashSet<&'a str>) {
    let head = command.split_whitespace().next().unwrap_or_default();
    match head {
        "cargo" => {
            required.insert("cargo");
            required.insert("rustc");
        }
        "bun" => {
            required.insert("bun");
            required.insert("node");
        }
        "pnpm" => {
            required.insert("pnpm");
            required.insert("node");
        }
        "npm" | "npx" => {
            required.insert("npm");
            required.insert("node");
        }
        "node" => {
            required.insert("node");
        }
        _ => {}
    }
}

fn tool_available(tool: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| {
        let candidate = dir.join(tool);
        if candidate.is_file() {
            return true;
        }
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{tool}.exe"));
            if exe.is_file() {
                return true;
            }
        }
        false
    })
}
