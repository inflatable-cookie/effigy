use super::prelude::*;

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
