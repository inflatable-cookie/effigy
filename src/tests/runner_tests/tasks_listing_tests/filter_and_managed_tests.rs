use crate::runner::tests::prelude::{
    assert_case_table, assert_output_excludes_all, assert_string_items_excludes_all,
    json_task_column, parse_json_output_with_schema_version, run_tasks_from_repo,
    setup_root_and_catalog_a_catalog, setup_root_with_catalog_tasks, temp_workspace,
    write_managed_dev_profile_manifest, ManagedProfileListingCase,
};

#[test]
fn run_tasks_with_task_filter_reports_only_matches() {
    let root = setup_root_and_catalog_a_catalog("task-filter");

    let out = run_tasks_from_repo(&root, Some("reset-db"), None, false);
    assert!(out.lines().any(|line| line == "Task Matches: reset-db"));
    assert!(out
        .lines()
        .any(|line| line == "- catalog_a/reset-db : catalog_a/effigy.toml"));
    assert!(out.lines().any(|line| line == "      printf catalog_a"));
    assert_eq!(
        out.lines()
            .filter(|line| line.starts_with("- "))
            .collect::<Vec<_>>(),
        ["- catalog_a/reset-db : catalog_a/effigy.toml"]
    );
}

#[test]
fn run_tasks_with_test_filter_shows_builtin_configuration_note() {
    let root = temp_workspace("task-filter-test-note");

    let out = run_tasks_from_repo(&root, Some("test"), None, false);
    for line in [
        "Task Matches: test",
        "Built-in Task Matches",
        "- test : Built-in test runner detection, supports <catalog>/test fallback, optional --plan",
        "• info: `test` is always built-in; configure explicit suites under `[test.suites]`",
    ] {
        assert!(out.lines().any(|actual| actual == line), "missing {line:?}: {out}");
    }
    assert_eq!(
        out.lines()
            .filter(|line| line.starts_with("- "))
            .collect::<Vec<_>>(),
        ["- test : Built-in test runner detection, supports <catalog>/test fallback, optional --plan"]
    );
}

#[test]
fn run_tasks_with_unmatched_filter_reports_no_matches_warning() {
    let root = temp_workspace("task-filter-no-matches");

    let out = run_tasks_from_repo(&root, Some("missing"), None, false);
    assert!(out.lines().any(|line| line == "Task Matches: missing"));
    assert!(out.lines().any(|line| line == "• warn: no matches"));
    assert_eq!(out.lines().filter(|line| line.starts_with("- ")).count(), 0);
    assert!(!out
        .lines()
        .any(|line| line.starts_with("Built-in Task Matches")));
    assert!(!out.lines().any(|line| line.starts_with("Resolution:")));
}

#[test]
fn run_tasks_with_builtin_only_filter_renders_builtin_matches_without_fallback_note() {
    let root = temp_workspace("task-filter-builtin-only");

    let out = run_tasks_from_repo(&root, Some("help"), None, false);
    assert!(out.lines().any(|line| line == "Task Matches: help"));
    assert!(out.lines().any(|line| line == "Built-in Task Matches"));
    assert!(out
        .lines()
        .any(|line| line == "- help : Show general help (same as --help)"));
    assert!(!out.lines().any(|line| {
        line == "• info: `test` is always built-in; configure explicit suites under `[test.suites]`"
    }));
    assert!(!out.lines().any(|line| line.starts_with("Resolution:")));
}

#[test]
fn run_tasks_with_filter_and_resolve_renders_resolution_probe_block() {
    let root = setup_root_with_catalog_tasks(
        "task-filter-resolve-probe",
        &[("catalog_a", &[("reset-db", "printf catalog_a-reset")])],
        false,
    );

    let out = run_tasks_from_repo(&root, Some("reset-db"), Some("catalog_a/reset-db"), false);
    for line in [
        "- catalog_a/reset-db : catalog_a/effigy.toml",
        "      printf catalog_a-reset",
        "Resolution: catalog_a/reset-db",
        "status: ok",
        "catalog: catalog_a",
        "task: reset-db",
    ] {
        assert!(
            out.lines().any(|actual| actual == line),
            "missing {line:?}: {out}"
        );
    }
}

#[test]
fn run_tasks_with_no_matches_and_resolve_still_renders_resolution_probe_block() {
    let root = temp_workspace("task-filter-no-matches-with-resolve");

    let out = run_tasks_from_repo(&root, Some("missing"), Some("missing"), false);
    for line in [
        "Task Matches: missing",
        "• warn: no matches",
        "Resolution: missing",
        "status: error",
    ] {
        assert!(
            out.lines().any(|actual| actual == line),
            "missing {line:?}: {out}"
        );
    }
    assert_eq!(out.lines().filter(|line| line.starts_with("- ")).count(), 0);
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
            assert_eq!(tasks, [format!("dev {}", case.profile)]);
            assert_string_items_excludes_all(&tasks, &["dev default".to_owned()]);
        } else {
            if case.filter.is_some() {
                assert!(out.lines().any(|line| line == "Task Matches: dev"));
            } else {
                assert!(out.lines().any(|line| line == "Tasks"));
            }
            let profile_row = format!("- dev {} : effigy.toml", case.profile);
            let profile_preview = format!("      <managed:tui profile:{}>", case.profile);
            assert!(
                out.lines()
                    .collect::<Vec<_>>()
                    .windows(2)
                    .any(|pair| pair == [profile_row.as_str(), profile_preview.as_str()]),
                "profile invocation and its managed preview must stay adjacent: {out}"
            );
            assert_output_excludes_all(&out, &["dev default"]);
        }
    });
}
