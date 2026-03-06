use super::prelude::{
    assert_case_table, assert_output_contains_all, assert_output_excludes_all,
    assert_string_items_contains_all, assert_string_items_excludes_all, json_task_column,
    parse_json_output_with_schema_version, run_tasks_from_repo, setup_root_and_farmyard_catalog,
    setup_root_with_catalog_tasks, temp_workspace, write_managed_dev_profile_manifest,
    ManagedProfileListingCase,
};

#[test]
fn run_tasks_with_task_filter_reports_only_matches() {
    let root = setup_root_and_farmyard_catalog("task-filter");

    let out = run_tasks_from_repo(&root, Some("reset-db"), None, false);
    assert_output_contains_all(&out, &["Task Matches: reset-db", "farmyard", "reset-db"]);
    assert_output_excludes_all(&out, &["root      │ reset-db"]);
}

#[test]
fn run_tasks_with_test_filter_shows_catalog_fallback_note() {
    let root = temp_workspace("task-filter-test-note");

    let out = run_tasks_from_repo(&root, Some("test"), None, false);
    assert_output_contains_all(
        &out,
        &[
            "Task Matches: test",
            "Built-in Task Matches",
            "built-in fallback supports `<catalog>/test`",
        ],
    );
}

#[test]
fn run_tasks_with_unmatched_filter_reports_no_matches_warning() {
    let root = temp_workspace("task-filter-no-matches");

    let out = run_tasks_from_repo(&root, Some("missing"), None, false);
    assert_output_contains_all(&out, &["Task Matches: missing", "no matches"]);
    assert_output_excludes_all(&out, &["Built-in Task Matches", "Resolution:"]);
}

#[test]
fn run_tasks_with_builtin_only_filter_renders_builtin_matches_without_fallback_note() {
    let root = temp_workspace("task-filter-builtin-only");

    let out = run_tasks_from_repo(&root, Some("help"), None, false);
    assert_output_contains_all(
        &out,
        &["Task Matches: help", "Built-in Task Matches", "help"],
    );
    assert_output_excludes_all(
        &out,
        &[
            "built-in fallback supports `<catalog>/test`",
            "Resolution:",
            "no matches",
        ],
    );
}

#[test]
fn run_tasks_with_filter_and_resolve_renders_resolution_probe_block() {
    let root = setup_root_with_catalog_tasks(
        "task-filter-resolve-probe",
        &[("farmyard", &[("reset-db", "printf farmyard-reset")])],
        false,
    );

    let out = run_tasks_from_repo(&root, Some("reset-db"), Some("farmyard/reset-db"), false);
    assert_output_contains_all(
        &out,
        &[
            "Task Matches: reset-db",
            "farmyard/reset-db",
            "Resolution: farmyard/reset-db",
            "status: ok",
            "catalog: farmyard",
            "task: reset-db",
        ],
    );
}

#[test]
fn run_tasks_with_no_matches_and_resolve_still_renders_resolution_probe_block() {
    let root = temp_workspace("task-filter-no-matches-with-resolve");

    let out = run_tasks_from_repo(&root, Some("missing"), Some("missing"), false);
    assert_output_contains_all(
        &out,
        &[
            "Task Matches: missing",
            "no matches",
            "Resolution: missing",
            "status: error",
        ],
    );
}

#[test]
fn run_tasks_lists_managed_profiles_with_invocation_labels() {
    let cases = [
        ManagedProfileListingCase {
            workspace: "tasks-managed-profiles-list",
            profile: "admin",
            filter: None,
            output_json: false,
            expected_field: "managed_profiles",
        },
        ManagedProfileListingCase {
            workspace: "tasks-managed-profiles-filter",
            profile: "front",
            filter: Some("dev"),
            output_json: false,
            expected_field: "managed_profile_matches",
        },
        ManagedProfileListingCase {
            workspace: "tasks-managed-profiles-json-list",
            profile: "admin",
            filter: None,
            output_json: true,
            expected_field: "managed_profiles",
        },
        ManagedProfileListingCase {
            workspace: "tasks-managed-profiles-json-filter",
            profile: "front",
            filter: Some("dev"),
            output_json: true,
            expected_field: "managed_profile_matches",
        },
    ];

    assert_case_table(cases, |case| {
        let root = temp_workspace(case.workspace);
        write_managed_dev_profile_manifest(&root, case.profile);
        let out = run_tasks_from_repo(&root, case.filter, None, case.output_json);

        if case.output_json {
            let expected_schema = if case.filter.is_some() {
                "effigy.tasks.filtered.v1"
            } else {
                "effigy.tasks.v1"
            };
            let parsed = parse_json_output_with_schema_version(&out, expected_schema, 1);
            let tasks = json_task_column(&parsed, case.expected_field);
            assert_string_items_contains_all(&tasks, &[format!("dev {}", case.profile)]);
            assert_string_items_excludes_all(&tasks, &["dev default".to_owned()]);
        } else {
            if case.filter.is_some() {
                assert_output_contains_all(
                    &out,
                    &["Task Matches: dev", &format!("dev {}", case.profile)],
                );
            } else {
                assert_output_contains_all(
                    &out,
                    &[
                        "Tasks",
                        &format!("dev {}", case.profile),
                        &format!("<managed:tui profile:{}>", case.profile),
                    ],
                );
            }
            assert_output_excludes_all(&out, &["dev default"]);
        }
    });
}
