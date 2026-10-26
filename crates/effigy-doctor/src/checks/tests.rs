use super::definitions::doctor_check_definitions;

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
            "pinned_sources",
            "god_files",
            "duplicate_blocks",
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
            "Doctor scan: god-files",
            "Doctor scan: duplicate-blocks",
            "Doctor task: health",
        ]
    );
}
