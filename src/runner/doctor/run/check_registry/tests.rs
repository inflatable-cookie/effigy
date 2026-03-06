use std::path::{Path, PathBuf};

use super::super::super::{DoctorState, ManifestSnapshot};
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
            "generated_assets",
            "attention_markers",
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
            run: noop,
        },
        DoctorCheckDefinition {
            name: "second",
            run: noop,
        },
        DoctorCheckDefinition {
            name: "third",
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
            "generated_assets",
            "attention_markers",
            "health_task",
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
