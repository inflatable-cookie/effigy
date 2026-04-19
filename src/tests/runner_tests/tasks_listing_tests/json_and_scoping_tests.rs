use crate::runner::tests::prelude::{
    assert_json_array_field, assert_json_array_field_non_empty, assert_json_string_field_eq,
    assert_output_contains_all, assert_output_excludes_all, assert_string_items_contains_all,
    create_workspace_dir, json_task_column, parse_json_output_with_schema_version, run_builtin_ok,
    run_tasks_from_repo, setup_root_and_catalog_a_catalog, setup_root_with_catalog_tasks,
    temp_workspace, write_root_manifest,
};

#[test]
fn run_tasks_json_renders_machine_readable_payload() {
    let root = setup_root_and_catalog_a_catalog("tasks-json");

    let out = run_tasks_from_repo(&root, None, None, true);

    let parsed = parse_json_output_with_schema_version(&out, "effigy.tasks.v1", 1);
    assert_eq!(parsed["catalog_count"], 2);
    assert_json_array_field(&parsed, "catalog_tasks");
    assert_json_array_field(&parsed, "managed_profiles");
    assert_json_array_field(&parsed, "builtin_tasks");
}

#[test]
fn run_tasks_json_includes_explicit_rows_for_empty_catalogs() {
    let root = setup_root_with_catalog_tasks(
        "tasks-json-empty-catalog-row",
        &[
            ("empty", &[]),
            ("catalog_a", &[("api", "printf catalog_a-api")]),
        ],
        false,
    );

    let out = run_tasks_from_repo(&root, None, None, true);

    let parsed = parse_json_output_with_schema_version(&out, "effigy.tasks.v1", 1);
    let catalog_rows = parsed["catalog_tasks"]
        .as_array()
        .expect("catalog_tasks array");
    assert!(
        catalog_rows.iter().any(|row| {
            row["task"].is_null()
                && row["run"].is_null()
                && row["manifest"]
                    .as_str()
                    .map(|manifest| manifest.ends_with("/empty/effigy.toml"))
                    .unwrap_or(false)
        }),
        "expected an explicit empty catalog row for the empty catalog manifest"
    );
    assert!(
        catalog_rows
            .iter()
            .any(|row| row["task"].as_str() == Some("catalog_a/api")),
        "expected non-empty catalog task rows to still be present"
    );
}

#[test]
fn run_tasks_json_preserves_catalog_row_order_with_empty_catalogs() {
    let root = setup_root_with_catalog_tasks(
        "tasks-json-empty-order",
        &[("alpha", &[("api", "printf alpha-api")]), ("beta", &[])],
        false,
    );

    let out = run_tasks_from_repo(&root, None, None, true);

    let parsed = parse_json_output_with_schema_version(&out, "effigy.tasks.v1", 1);
    let catalog_rows = parsed["catalog_tasks"]
        .as_array()
        .expect("catalog_tasks array");
    assert!(
        catalog_rows.len() >= 3,
        "expected rows for root plus each child catalog"
    );
    let child_rows = catalog_rows
        .iter()
        .filter(|row| {
            row["manifest"]
                .as_str()
                .map(|manifest| !manifest.ends_with("/effigy.toml") || manifest.contains("/alpha/") || manifest.contains("/beta/"))
                .unwrap_or(false)
        })
        .filter(|row| {
            row["manifest"]
                .as_str()
                .map(|manifest| manifest.ends_with("/alpha/effigy.toml") || manifest.ends_with("/beta/effigy.toml"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    assert_eq!(child_rows.len(), 2, "expected one row per child catalog");
    assert_eq!(child_rows[0]["task"].as_str(), Some("alpha/api"));
    assert!(child_rows[1]["task"].is_null());
    assert!(
        child_rows[1]["manifest"]
            .as_str()
            .map(|manifest| manifest.ends_with("/beta/effigy.toml"))
            .unwrap_or(false),
        "expected second row to remain the empty beta catalog"
    );
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
fn run_tasks_json_prefixed_test_filter_skips_builtin_fallback_note() {
    let root = setup_root_with_catalog_tasks(
        "tasks-json-prefixed-test-filter",
        &[("catalog_a", &[("test", "printf catalog_a-test")])],
        false,
    );

    let out = run_tasks_from_repo(&root, Some("catalog_a/test"), None, true);

    let parsed = parse_json_output_with_schema_version(&out, "effigy.tasks.filtered.v1", 1);
    assert_json_string_field_eq(&parsed, "filter", "catalog_a/test");
    assert_json_array_field(&parsed, "builtin_matches");
    assert_eq!(
        parsed["builtin_matches"]
            .as_array()
            .expect("builtin_matches array")
            .len(),
        0
    );
    assert_eq!(
        parsed["notes"].as_array().expect("notes array").len(),
        0,
        "prefixed selectors should not report built-in fallback notes"
    );
    assert_string_items_contains_all(
        &json_task_column(&parsed, "matches"),
        &["catalog_a/test".to_owned()],
    );
}

#[test]
fn run_manifest_task_prefixed_builtin_tasks_targets_catalog_root_only() {
    let root = setup_root_with_catalog_tasks(
        "builtin-tasks-prefixed-catalog",
        &[
            ("catalog_a", &[("api", "printf catalog_a-api")]),
            ("catalog_b", &[("admin", "printf catalog_b-admin")]),
        ],
        false,
    );
    write_root_manifest(&root, "[tasks.root-only]\nrun = \"printf root\"\n");

    let out = run_builtin_ok(root, "catalog_a/tasks", &[]);

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
    let catalog_b = create_workspace_dir(&root, "catalog_b");
    write_root_manifest(&root, "[tasks.root-only]\nrun = \"printf root\"\n");

    let out = run_builtin_ok(catalog_b, "../froyo/tasks", &[]);

    assert_output_contains_all(&out, &["Catalogs", "count: 1", "validate"]);
    assert_output_excludes_all(&out, &["root-only"]);
}
