use std::path::{Path, PathBuf};

use super::super::super::report::{DoctorState, ManifestSnapshot};
use super::definitions::{doctor_check_definitions, DoctorCheckContext, DoctorCheckDefinition};
use super::executor::for_each_check;

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
    let _context = DoctorCheckContext::new(root, &manifest);

    let mut visited = Vec::<&str>::new();
    for_each_check(doctor_check_definitions(), |check| visited.push(check.name));
    assert_eq!(
        visited,
        vec![
            "manifest_conflicts",
            "environment_tools",
            "task_references",
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
fn doctor_check_registry_scan_progress_labels_are_stable() {
    let labels = doctor_check_definitions()
        .iter()
        .filter_map(|check| check.progress_label)
        .collect::<Vec<&str>>();
    assert_eq!(
        labels,
        vec![
            "Doctor scan: god-files",
            "Doctor scan: duplicate-blocks",
            "Doctor scan: comment-ratio",
            "Doctor scan: generated-assets",
            "Doctor scan: generated-in-src",
            "Doctor scan: attention-markers",
            "Doctor scan: stale-suppressions",
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
