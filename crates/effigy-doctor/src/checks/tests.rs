use super::definitions::doctor_check_definitions;

/// The built-in check registry is a stable ordered surface: the run order and
/// the progress labels downstream surfaces parse must not drift silently.
#[test]
fn doctor_check_registry_order_and_progress_labels_are_stable() {
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
            "pinned_sources",
            "god_files",
            "duplicate_blocks",
            "health_task",
        ]
    );

    let labels = doctor_check_definitions()
        .iter()
        .filter_map(|check| check.progress_label.map(|label| label.active))
        .collect::<Vec<&str>>();
    assert_eq!(
        labels,
        vec![
            "scanning for god files",
            "scanning for duplicate blocks",
            "running health task",
        ]
    );
}
