use super::prelude::*;

#[test]
fn run_tasks_json_renders_machine_readable_payload() {
    let root = setup_root_and_farmyard_catalog("tasks-json");

    let out = run_tasks_from_repo(&root, None, None, true);

    let parsed = parse_json_output(&out);
    assert_eq!(parsed["catalog_count"], 2);
    assert!(parsed["catalog_tasks"].is_array());
    assert!(parsed["managed_profiles"].is_array());
    assert!(parsed["builtin_tasks"].is_array());
}

#[test]
fn run_tasks_json_filter_includes_builtin_matches_and_notes() {
    let root = temp_workspace("tasks-json-filter");
    let out = run_tasks_from_repo(&root, Some("test"), None, true);

    let parsed = parse_json_output(&out);
    assert_eq!(parsed["filter"], "test");
    assert!(parsed["builtin_matches"].is_array());
    assert!(parsed["managed_profile_matches"].is_array());
    assert!(parsed["notes"]
        .as_array()
        .is_some_and(|items| !items.is_empty()));
}

#[test]
fn run_manifest_task_prefixed_builtin_tasks_targets_catalog_root_only() {
    let root = setup_root_with_catalog_tasks(
        "builtin-tasks-prefixed-catalog",
        &[
            ("farmyard", &[("api", "printf farmyard-api")]),
            ("dairy", &[("admin", "printf dairy-admin")]),
        ],
        false,
    );
    write_root_manifest(&root, "[tasks.root-only]\nrun = \"printf root\"\n");

    let out = run_builtin_ok(root, "farmyard/tasks", &[]);

    assert_contains_all(&out, &["Catalogs", "count: 1", "api"]);
    assert!(!out.contains("admin"));
    assert!(!out.contains("root-only"));
}

#[test]
fn run_manifest_task_relative_prefixed_builtin_tasks_target_catalog_root_only() {
    let root = setup_root_with_catalog_tasks(
        "builtin-tasks-relative-prefixed-catalog",
        &[("froyo", &[("validate", "printf froyo-validate")])],
        false,
    );
    let dairy = create_workspace_dir(&root, "dairy");
    write_root_manifest(&root, "[tasks.root-only]\nrun = \"printf root\"\n");

    let out = run_builtin_ok(dairy, "../froyo/tasks", &[]);

    assert_contains_all(&out, &["Catalogs", "count: 1", "validate"]);
    assert!(!out.contains("root-only"));
}
