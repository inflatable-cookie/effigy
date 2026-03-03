use super::super::prelude::*;

#[test]
fn run_manifest_task_managed_tui_supports_concurrent_entries() {
    let _guard = lock_test();
    let root = temp_workspace("managed-concurrent-entries");
    let _env = managed_tui_env();
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { task = "api", start = 1, tab = 3 },
  { run = "printf background", start = 2, tab = 2, start_after_ms = 250 },
  { task = "front", start = 3, tab = 1 }
]

[tasks.api]
run = "printf api"

[tasks.front]
run = "printf front"
"#,
    );

    assert_run_dev_with_repo_contains(
        &root,
        &[],
        &[
            "Managed Task Plan",
            "profile: default",
            "tab-order: front, process-2, api",
            "printf api",
            "printf background",
            "printf front",
            "250",
        ],
    );
}

#[test]
fn run_manifest_task_managed_tui_rejects_concurrent_entry_with_both_task_and_run() {
    let root = temp_workspace("managed-concurrent-invalid-entry");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { task = "api", run = "printf oops", start = 1, tab = 1 }
]

[tasks.api]
run = "printf api"
"#,
    );

    let err = run_dev_with_repo(&root, &[]).expect_err("invalid concurrent entry should fail");
    assert_managed_process_invalid_definition(err, "dev", "api", Some("either `task` or `run`"));
}

#[test]
fn run_manifest_task_managed_tui_supports_ranked_tab_order_map() {
    let _guard = lock_test();
    let _env = managed_tui_env();
    let cases = [
        ManagedPlanCase {
            workspace: "managed-tab-order",
        },
        ManagedPlanCase {
            workspace: "managed-tab-order-ranked",
        },
    ];

    for case in cases {
        let root = temp_workspace(case.workspace);
        write_root_manifest(
            &root,
            r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "printf api", start = 1, tab = 3 },
  { name = "jobs", run = "printf jobs", start = 2, tab = 4 },
  { name = "cream", run = "printf cream", start = 3, tab = 2 },
  { name = "dairy", run = "printf dairy", start = 4, tab = 1 }
]
"#,
        );

        let out = run_dev_with_repo(&root, &[]).expect("managed plan should render");
        assert_contains_all(&out, &["tab-order: dairy, cream, api, jobs"]);
    }
}

#[test]
fn run_manifest_task_managed_tui_supports_ranked_tab_order_map_with_task_refs() {
    let _guard = lock_test();
    let root = temp_workspace("managed-tab-order-ranked-refs");
    let _env = managed_tui_env();
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { task = "farmyard/api", start = 1, tab = 3 },
  { task = "farmyard/jobs", start = 2, tab = 4 },
  { task = "cream/dev", start = 3, tab = 2 },
  { task = "dairy/dev", start = 4, tab = 1 }
]
"#,
    );
    write_catalogs_with_tasks(
        &root,
        &[
            (
                "farmyard",
                &[
                    ("api", "printf farmyard-api"),
                    ("jobs", "printf farmyard-jobs"),
                ],
            ),
            ("cream", &[("dev", "printf cream-dev")]),
            ("dairy", &[("dev", "printf dairy-dev")]),
        ],
    );

    assert_run_dev_with_repo_contains(
        &root,
        &[],
        &["tab-order: dairy/dev, cream/dev, farmyard/api, farmyard/jobs"],
    );
}

#[test]
fn run_manifest_task_managed_tui_supports_single_definition_ordered_profile_entries() {
    let _guard = lock_test();
    let root = temp_workspace("managed-single-definition-ordered-profile");
    let _env = managed_tui_env();
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { task = "farmyard/api", start = 1, tab = 3 },
  { task = "farmyard/jobs", start = 2, tab = 4, start_after_ms = 1200 },
  { task = "cream/dev", start = 3, tab = 2 },
  { task = "dairy/dev", start = 4, tab = 1 }
]
"#,
    );
    write_catalogs_with_tasks(
        &root,
        &[
            (
                "farmyard",
                &[
                    ("api", "printf farmyard-api"),
                    ("jobs", "printf farmyard-jobs"),
                ],
            ),
            ("cream", &[("dev", "printf cream-dev")]),
            ("dairy", &[("dev", "printf dairy-dev")]),
        ],
    );

    assert_run_dev_with_repo_contains(
        &root,
        &[],
        &[
            "tab-order: dairy/dev, cream/dev, farmyard/api, farmyard/jobs",
            "start-after-ms",
            "1200",
        ],
    );
}

#[test]
fn run_manifest_task_managed_tui_errors_when_concurrent_entry_missing_task_and_run() {
    let root = temp_workspace("managed-tab-order-invalid");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "jobs" }]
"#,
    );

    let err = run_dev_with_repo(&root, &[]).expect_err("invalid concurrent entry should fail");
    assert_managed_process_invalid_definition(
        err,
        "dev",
        "jobs",
        Some("missing both `task` and `run`"),
    );
}

#[test]
fn run_manifest_task_managed_tui_errors_when_process_has_run_and_task() {
    let root = temp_workspace("managed-invalid-process-def");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "api", run = "printf api", task = "api" }]
"#,
    );

    let err = run_dev_with_repo(&root, &[]).expect_err("invalid process definition should fail");
    assert_managed_process_invalid_definition(err, "dev", "api", None);
}
