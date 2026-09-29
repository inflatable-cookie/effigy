use std::cell::RefCell;
use std::path::{Path, PathBuf};

use effigy_cli::TaskInvocation;
use effigy_manifest::{load_task_manifest, DeferredCommand, LoadedCatalog, TaskManifest};
use effigy_tasks::TaskSelector;

use super::*;
use crate::{check_id, DoctorRuntimeDiagnostics, DoctorRuntimePorts, DoctorSeverity};

struct RecordingPorts {
    calls: RefCell<usize>,
}

impl RecordingPorts {
    fn new() -> Self {
        Self {
            calls: RefCell::new(0),
        }
    }

    fn calls(&self) -> usize {
        *self.calls.borrow()
    }
}

impl DoctorRuntimePorts for RecordingPorts {
    fn run_manifest_task(
        &self,
        _invocation: &TaskInvocation,
        _cwd: PathBuf,
    ) -> Result<String, DoctorError> {
        *self.calls.borrow_mut() += 1;
        Ok(
            r#"{"schema":"effigy.task.run.v1","stdout":"healthy","stderr":"","exit_code":0}"#
                .to_owned(),
        )
    }

    fn select_deferral(
        &self,
        _selector: &TaskSelector,
        _catalogs: &[LoadedCatalog],
        _cwd: &Path,
        _workspace_root: &Path,
    ) -> Option<DeferredCommand> {
        None
    }

    fn runtime_diagnostics(
        &self,
        _resolved_root: &Path,
    ) -> Result<DoctorRuntimeDiagnostics, DoctorError> {
        Ok(DoctorRuntimeDiagnostics::default())
    }
}

fn temp_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "effigy-doctor-health-task-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("mkdir root");
    root
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(path, contents).expect("write");
}

fn loaded_catalog(catalog_root: &Path) -> LoadedCatalog {
    let manifest_path = catalog_root.join("effigy.toml");
    let manifest: TaskManifest = load_task_manifest(&manifest_path).expect("manifest");
    LoadedCatalog {
        alias: "child".to_owned(),
        catalog_root: catalog_root.to_path_buf(),
        manifest_path,
        bundle_root: None,
        manifest,
        defer_run: None,
        deferred_builtins: Default::default(),
        depth: 1,
        draft_sources: Default::default(),
    }
}

fn write_child_manifest(catalog_root: &Path) {
    write(
        &catalog_root.join("effigy.toml"),
        "[package_manager]\njs = \"bun\"\n\n[tasks]\nhealth = \"printf healthy\"\n",
    );
}

#[test]
fn missing_child_bootstrap_emits_finding_and_skips_health() {
    let root = temp_root("gap");
    let workspace = root.join("workspace");
    let child = workspace.join("child");
    std::fs::create_dir_all(workspace.join(".git")).expect("git boundary");
    write_child_manifest(&child);
    write(
        &child.join("package.json"),
        r#"{"dependencies":{"left-pad":"1.3.0"}}"#,
    );
    write(&child.join("bun.lock"), "");
    // The parent workspace provides the same package.
    std::fs::create_dir_all(workspace.join("node_modules")).expect("parent install");

    let catalog = loaded_catalog(&child);
    let ports = RecordingPorts::new();
    let mut state = DoctorState::new();
    check_health_task(&child, &workspace, &[catalog], &mut state, &ports, None);

    assert_eq!(
        ports.calls(),
        0,
        "health task must not run before bootstrap"
    );
    let bootstrap = state
        .findings
        .iter()
        .find(|finding| finding.check_id == check_id::HEALTH_TASK_BOOTSTRAP)
        .expect("bootstrap finding");
    assert_eq!(bootstrap.severity, DoctorSeverity::Error);
    assert!(bootstrap
        .remediation
        .contains("effigy bootstrap deps sync child"));
    assert!(!state
        .findings
        .iter()
        .any(|finding| finding.check_id == check_id::HEALTH_TASK_EXECUTE));

    std::fs::remove_dir_all(root).ok();
}

#[test]
fn bootstrapped_child_runs_health() {
    let root = temp_root("bootstrapped");
    let workspace = root.join("workspace");
    let child = workspace.join("child");
    std::fs::create_dir_all(workspace.join(".git")).expect("git boundary");
    write_child_manifest(&child);
    write(
        &child.join("package.json"),
        r#"{"dependencies":{"left-pad":"1.3.0"}}"#,
    );
    write(&child.join("bun.lock"), "");
    write_package(&child.join("node_modules/left-pad"), "left-pad");

    let catalog = loaded_catalog(&child);
    let ports = RecordingPorts::new();
    let mut state = DoctorState::new();
    check_health_task(&child, &workspace, &[catalog], &mut state, &ports, None);

    assert_eq!(ports.calls(), 1, "bootstrapped child must run health");
    assert!(state
        .findings
        .iter()
        .any(|finding| finding.check_id == check_id::HEALTH_TASK_EXECUTE));
    assert!(!state
        .findings
        .iter()
        .any(|finding| finding.check_id == check_id::HEALTH_TASK_BOOTSTRAP));

    std::fs::remove_dir_all(root).ok();
}

#[test]
fn non_js_child_runs_health_without_bootstrap() {
    let root = temp_root("non-js");
    let workspace = root.join("workspace");
    let child = workspace.join("child");
    std::fs::create_dir_all(workspace.join(".git")).expect("git boundary");
    write(
        &child.join("effigy.toml"),
        "[tasks]\nhealth = \"printf healthy\"\n",
    );
    write(
        &child.join("package.json"),
        r#"{"dependencies":{"left-pad":"1.3.0"}}"#,
    );
    write(&child.join("bun.lock"), "");

    let catalog = loaded_catalog(&child);
    let ports = RecordingPorts::new();
    let mut state = DoctorState::new();
    check_health_task(&child, &workspace, &[catalog], &mut state, &ports, None);

    assert_eq!(ports.calls(), 1, "non-JS catalog must still run health");
    assert!(!state
        .findings
        .iter()
        .any(|finding| finding.check_id == check_id::HEALTH_TASK_BOOTSTRAP));

    std::fs::remove_dir_all(root).ok();
}

fn write_package(dir: &Path, name: &str) {
    write(
        &dir.join("package.json"),
        &format!(r#"{{"name":"{name}","version":"1.0.0","exports":"./entry.cjs"}}"#),
    );
    write(&dir.join("entry.cjs"), "module.exports = true;");
}

fn node_resolve(child: &Path, name: &str) -> PathBuf {
    let output = std::process::Command::new("node")
        .args([
            "-e",
            "process.stdout.write(require.resolve(process.argv[1]))",
            name,
        ])
        .current_dir(child)
        .output()
        .expect("Node is required for package-resolution regressions");
    assert!(output.status.success(), "Node resolution: {output:?}");
    PathBuf::from(String::from_utf8(output.stdout).expect("Node path"))
}

#[cfg(unix)]
#[test]
fn installed_package_evidence_guards_health_against_parent_resolution() {
    for layout in [
        "empty-entry",
        "parent-link",
        "local",
        "pnpm-store",
        "bun-store",
        "shared",
        "shared-isolated",
        "pnpm-shared-isolated",
        "shared-isolated-root-link",
    ] {
        let root = temp_root(layout);
        let workspace = root.join("workspace");
        let child = workspace.join("child");
        let name = "@fixture/health-package";
        std::fs::create_dir_all(workspace.join(".git")).expect("workspace boundary");
        write_child_manifest(&child);
        write(
            &child.join("package.json"),
            &format!(r#"{{"name":"child","dependencies":{{"{name}":"1.0.0"}}}}"#),
        );
        let parent_package = workspace.join("node_modules").join(name);
        let child_package = child.join("node_modules").join(name);
        write_package(&parent_package, name);
        let invalid = matches!(
            layout,
            "empty-entry" | "parent-link" | "shared-isolated-root-link"
        );
        let expected = match layout {
            "shared" | "shared-isolated" | "shared-isolated-root-link" | "pnpm-shared-isolated" => {
                write(
                    &workspace.join("package.json"),
                    r#"{"workspaces":["child"]}"#,
                );
                write(&workspace.join("bun.lock"), "");
                if layout == "pnpm-shared-isolated" {
                    write(
                        &child.join("effigy.toml"),
                        "[package_manager]\njs = \"pnpm\"\n[tasks]\nhealth = \"printf healthy\"\n",
                    );
                    write(&workspace.join("pnpm-lock.yaml"), "");
                    write(
                        &workspace.join("pnpm-workspace.yaml"),
                        "packages:\n  - child\n",
                    );
                }
                if layout.starts_with("shared-isolated") || layout == "pnpm-shared-isolated" {
                    write(
                        &workspace.join("bunfig.toml"),
                        "[install]\nlinker = \"isolated\"\n",
                    );
                    if layout == "shared-isolated-root-link" {
                        std::os::unix::fs::symlink(
                            workspace.join("node_modules"),
                            child.join("node_modules"),
                        )
                        .expect("borrow root modules");
                    }
                    let store_dir = if layout == "pnpm-shared-isolated" {
                        ".pnpm"
                    } else {
                        ".bun"
                    };
                    let store = workspace
                        .join("node_modules")
                        .join(store_dir)
                        .join("fixture/node_modules")
                        .join(name);
                    write_package(&store, name);
                    if layout == "shared-isolated-root-link" {
                        parent_package.clone()
                    } else {
                        std::fs::create_dir_all(child_package.parent().unwrap())
                            .expect("scope directory");
                        std::os::unix::fs::symlink(&store, &child_package)
                            .expect("workspace store link");
                        store
                    }
                } else {
                    parent_package.clone()
                }
            }
            "pnpm-store" | "bun-store" => {
                std::fs::create_dir_all(child.join(".git")).expect("child boundary");
                if layout == "pnpm-store" {
                    write(
                        &child.join("effigy.toml"),
                        "[package_manager]\njs = \"pnpm\"\n[tasks]\nhealth = \"printf healthy\"\n",
                    );
                }
                let manager_store = if layout == "pnpm-store" {
                    ".pnpm"
                } else {
                    ".bun"
                };
                let store = child
                    .join("node_modules")
                    .join(manager_store)
                    .join("fixture/node_modules")
                    .join(name);
                write_package(&store, name);
                std::fs::create_dir_all(child_package.parent().unwrap()).expect("scope directory");
                std::os::unix::fs::symlink(&store, &child_package).expect("local store link");
                store
            }
            "local" => {
                std::fs::create_dir_all(child.join(".git")).expect("child boundary");
                write_package(&child_package, name);
                child_package.clone()
            }
            _ => {
                std::fs::create_dir_all(child.join(".git")).expect("child boundary");
                std::fs::create_dir_all(child_package.parent().unwrap()).expect("scope directory");
                if layout == "empty-entry" {
                    std::fs::create_dir_all(&child_package).expect("empty package");
                } else {
                    std::os::unix::fs::symlink(&parent_package, &child_package)
                        .expect("parent package link");
                }
                parent_package.clone()
            }
        };
        assert_eq!(
            node_resolve(&child, name),
            std::fs::canonicalize(expected.join("entry.cjs")).expect("expected path"),
            "{layout}"
        );
        let catalog = loaded_catalog(&child);
        let ports = RecordingPorts::new();
        let mut state = DoctorState::new();
        check_health_task(&child, &workspace, &[catalog], &mut state, &ports, None);
        assert_eq!(ports.calls(), usize::from(!invalid), "{layout}");
        let finding = state
            .findings
            .iter()
            .find(|finding| finding.check_id == check_id::HEALTH_TASK_BOOTSTRAP);
        assert_eq!(finding.is_some(), invalid, "{layout}: {:?}", state.findings);
        if let Some(finding) = finding {
            assert!(finding.evidence.contains(name), "{finding:?}");
            assert!(
                finding.remediation.contains("effigy bootstrap deps sync"),
                "{finding:?}"
            );
        }
        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
