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
    std::fs::create_dir_all(child.join("node_modules").join("left-pad")).expect("child install");

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
