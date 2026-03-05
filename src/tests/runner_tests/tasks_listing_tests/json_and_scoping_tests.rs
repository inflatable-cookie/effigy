use super::prelude::*;

#[test]
fn run_tasks_json_renders_machine_readable_payload() {
    let root = setup_root_and_farmyard_catalog("tasks-json");

    let out = run_tasks_from_repo(&root, None, None, true);

    let parsed = parse_json_output_with_schema_version(&out, "effigy.tasks.v1", 1);
    assert_eq!(parsed["catalog_count"], 2);
    assert_json_array_field(&parsed, "catalog_tasks");
    assert_json_array_field(&parsed, "managed_profiles");
    assert_json_array_field(&parsed, "builtin_tasks");
}

#[test]
fn run_tasks_json_filter_includes_builtin_matches_and_notes() {
    let root = temp_workspace("tasks-json-filter");
    let out = run_tasks_from_repo(&root, Some("test"), None, true);

    let parsed = parse_json_output_with_schema_version(&out, "effigy.tasks.filtered.v1", 1);
    assert_json_string_field_eq(&parsed, "filter", "test");
    assert_json_array_field(&parsed, "builtin_matches");
    assert_json_array_field(&parsed, "managed_profile_matches");
    assert_json_array_field_non_empty(&parsed, "notes");
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

    assert_output_contains_all(&out, &["Catalogs", "count: 1", "api"]);
    assert_output_excludes_all(&out, &["admin", "root-only"]);
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

    assert_output_contains_all(&out, &["Catalogs", "count: 1", "validate"]);
    assert_output_excludes_all(&out, &["root-only"]);
}
