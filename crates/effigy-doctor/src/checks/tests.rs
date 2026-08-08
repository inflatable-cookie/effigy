use std::path::{Path, PathBuf};

use effigy_cli::TaskInvocation;
use effigy_manifest::{DeferredCommand, LoadedCatalog};
use effigy_tasks::TaskSelector;

use super::definitions::{doctor_check_definitions, DoctorCheckContext, DoctorCheckDefinition};
use super::executor::for_each_check;
use crate::{
    manifest_snapshot::ManifestSnapshot, DoctorError, DoctorRuntimeDiagnostics, DoctorRuntimePorts,
    DoctorState,
};

struct StubPorts;

impl DoctorRuntimePorts for StubPorts {
    fn run_manifest_task(
        &self,
        _invocation: &TaskInvocation,
        _cwd: PathBuf,
    ) -> Result<String, DoctorError> {
        Ok(String::new())
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

#[test]
fn doctor_check_registry_order_is_stable() {
    let order = doctor_check_definitions()
        .iter()
        .map(|check| check.name)
        .collect::<Vec<&str>>();
    assert_eq!(
        order,
        vec![
            "manifest_conflicts",
            "environment_tools",
            "task_references",
            "graph_index",
            "god_files",
            "duplicate_blocks",
            "comment_ratio",
            "generated_assets",
            "generated_in_src",
            "attention_markers",
            "stale_suppressions",
            "health_task",
        ]
    );
}

#[test]
fn doctor_check_executor_visits_definitions_in_declaration_order() {
    fn noop(_: &DoctorCheckContext<'_>, _: &mut DoctorState) {}

    let checks = [
        DoctorCheckDefinition {
            name: "first",
            progress_label: None,
            run: noop,
        },
        DoctorCheckDefinition {
            name: "second",
            progress_label: None,
            run: noop,
        },
        DoctorCheckDefinition {
            name: "third",
            progress_label: None,
            run: noop,
        },
    ];
    let mut visited = Vec::<&str>::new();
    for_each_check(&checks, |check| visited.push(check.name));
    assert_eq!(visited, vec!["first", "second", "third"]);
}

#[test]
fn doctor_check_registry_is_executor_composable_without_control_flow_changes() {
    let root = Path::new("/tmp/doctor-workspace");
    let manifest = empty_manifest_snapshot();
    let ports = StubPorts;
    let _context = DoctorCheckContext::new(root, &manifest, &ports);

    let mut visited = Vec::<&str>::new();
    for_each_check(doctor_check_definitions(), |check| visited.push(check.name));
    assert_eq!(
        visited,
        vec![
            "manifest_conflicts",
            "environment_tools",
            "task_references",
            "graph_index",
            "god_files",
            "duplicate_blocks",
            "comment_ratio",
            "generated_assets",
            "generated_in_src",
            "attention_markers",
            "stale_suppressions",
            "health_task",
        ]
    );
}

#[test]
fn doctor_check_registry_progress_labels_are_stable() {
    let labels = doctor_check_definitions()
        .iter()
        .filter_map(|check| check.progress_label)
        .collect::<Vec<&str>>();
    assert_eq!(
        labels,
        vec![
            "Doctor check: graph index",
            "Doctor scan: god-files",
            "Doctor scan: duplicate-blocks",
            "Doctor scan: comment-ratio",
            "Doctor scan: generated-assets",
            "Doctor scan: generated-in-src",
            "Doctor scan: attention-markers",
            "Doctor scan: stale-suppressions",
            "Doctor task: health",
        ]
    );
}

fn empty_manifest_snapshot() -> ManifestSnapshot {
    ManifestSnapshot {
        manifest_paths: vec![PathBuf::from("/tmp/doctor-workspace/effigy.toml")],
        parsed_catalogs: Vec::new(),
        preferred_js_pm: None,
        parse_ok_any: true,
    }
}
