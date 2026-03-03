use super::prelude::*;

#[test]
fn run_tasks_with_task_filter_reports_only_matches() {
    let root = setup_root_and_farmyard_catalog("task-filter");

    let out = run_tasks_from_repo(&root, Some("reset-db"), None, false);
    assert_contains_all(&out, &["Task Matches: reset-db", "farmyard", "reset-db"]);
    assert!(!out.contains("root      │ reset-db"));
}

#[test]
fn run_tasks_with_test_filter_shows_catalog_fallback_note() {
    let root = temp_workspace("task-filter-test-note");

    let out = run_tasks_from_repo(&root, Some("test"), None, false);
    assert_contains_all(
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

    for case in cases {
        let root = temp_workspace(case.workspace);
        write_managed_dev_profile_manifest(&root, case.profile);
        let out = run_tasks_from_repo(&root, case.filter, None, case.output_json);

        if case.output_json {
            let parsed = parse_json_output(&out);
            let tasks = json_task_column(&parsed, case.expected_field);
            assert!(tasks.contains(&format!("dev {}", case.profile)));
            assert!(!tasks.contains(&"dev default".to_owned()));
        } else {
            if case.filter.is_some() {
                assert_contains_all(
                    &out,
                    &["Task Matches: dev", &format!("dev {}", case.profile)],
                );
            } else {
                assert_contains_all(
                    &out,
                    &[
                        "Tasks",
                        &format!("dev {}", case.profile),
                        &format!("<managed:tui profile:{}>", case.profile),
                    ],
                );
            }
            assert!(!out.contains("dev default"));
        }
    }
}
