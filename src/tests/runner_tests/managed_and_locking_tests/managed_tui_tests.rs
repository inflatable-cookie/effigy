use super::prelude::*;

#[test]
fn run_manifest_task_managed_tui_uses_default_profile_when_not_specified() {
    let _guard = lock_test();
    let root = temp_workspace("managed-default-profile");
    let _env = managed_tui_env();
    write_managed_admin_profile_manifest(&root);

    let out = run_dev_with_repo(&root, &[]).expect("managed plan should render");
    assert_contains_all(
        &out,
        &[
            "Managed Task Plan",
            "profile: default",
            "api",
            "front",
            "admin",
            "fail-on-non-zero: enabled",
        ],
    );
}

#[test]
fn run_manifest_task_managed_tui_accepts_named_profile_argument() {
    let _guard = lock_test();
    let root = temp_workspace("managed-named-profile");
    let _env = managed_tui_env();
    write_managed_admin_profile_manifest(&root);

    let out = run_dev(&root, &["admin"]).expect("managed plan should render");
    assert_contains_all(&out, &["profile: admin", "api", "admin"]);
    assert!(!out.contains("front"));
}

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
fn run_manifest_task_managed_tui_supports_profile_specific_concurrent_entries() {
    let _guard = lock_test();
    let root = temp_workspace("managed-concurrent-profile-specific");
    let _env = managed_tui_env();
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { run = "printf default-api", start = 1, tab = 2 },
  { run = "printf default-front", start = 2, tab = 1 }
]

[tasks.dev.profiles.admin]
concurrent = [
  { run = "printf admin-api", start = 1, tab = 1 }
]
"#,
    );

    let out_default = run_dev_with_repo(&root, &[]).expect("default managed plan should render");
    assert_contains_all(
        &out_default,
        &["profile: default", "default-api", "default-front"],
    );
    assert!(!out_default.contains("admin-api"));

    let out_admin = run_dev(&root, &["admin"]).expect("admin managed plan should render");
    assert_contains_all(&out_admin, &["profile: admin", "admin-api"]);
    assert!(!out_admin.contains("default-front"));
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
fn run_manifest_task_managed_tui_errors_for_unknown_profile() {
    let root = temp_workspace("managed-unknown-profile");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "api", run = "cargo run -p api" }]
"#,
    );

    let err = run_dev(&root, &["admin"]).expect_err("unknown profile should fail");
    assert_managed_profile_not_found(err, "dev", "admin", &["default"]);
}

#[test]
fn run_manifest_task_managed_tui_processes_can_reference_other_tasks() {
    let _guard = lock_test();
    let root = temp_workspace("managed-task-refs");
    let _env = managed_tui_env();
    let farmyard = root.join("farmyard");
    let cream = root.join("cream");

    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", task = "farmyard/api" },
  { name = "front", task = "cream/dev" }
]
"#,
    );
    write_catalogs_with_tasks(
        &root,
        &[
            ("farmyard", &[("api", "printf farmyard-api")]),
            ("cream", &[("dev", "printf cream-dev")]),
        ],
    );

    let out = run_dev(&root, &[]).expect("managed plan should render");

    assert_contains_all(
        &out,
        &[
            "farmyard-api",
            "cream-dev",
            &farmyard.display().to_string(),
            &cream.display().to_string(),
        ],
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

#[test]
fn run_manifest_task_managed_tui_supports_compact_profile_task_refs() {
    let _guard = lock_test();
    let root = temp_workspace("managed-compact-profile-refs");
    let _env = managed_tui_env();
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ task = "farmyard/api" }, { task = "cream/dev" }]

[tasks.dev.profiles.admin]
concurrent = [{ task = "farmyard/api" }]
"#,
    );
    write_catalogs_with_tasks(
        &root,
        &[
            ("farmyard", &[("api", "printf farmyard-api")]),
            ("cream", &[("dev", "printf cream-dev")]),
        ],
    );

    let out = run_dev_with_repo(&root, &[]).expect("managed compact plan should render");
    assert_contains_all(
        &out,
        &[
            "profile: default",
            "farmyard-api",
            "cream-dev",
            "farmyard/api",
            "cream/dev",
        ],
    );
}

#[test]
fn run_manifest_task_managed_tui_process_run_array_supports_task_refs() {
    let _guard = lock_test();
    let root = temp_workspace("managed-process-run-array");
    let farmyard = create_workspace_dir(&root, "farmyard");
    let _env = managed_tui_env();

    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "combo", task = "combo" }]

[tasks.combo]
run = ["printf start", { task = "farmyard/api" }, "printf done"]
"#,
    );
    write_catalog_tasks(
        &farmyard,
        Some("farmyard"),
        &[("api", "printf farmyard-api")],
    );

    let out = run_dev_with_repo(&root, &[]).expect("managed plan should render");
    assert_contains_all(&out, &["printf start", "farmyard-api", "printf done", "cd"]);
}

#[test]
fn run_manifest_task_managed_tui_rejects_invalid_task_ref_syntax() {
    let _guard = lock_test();
    let _env = managed_tui_env();
    let cases = [
        ManagedTaskRefInvalidCase {
            workspace: "managed-compact-profile-ref-unterminated-quote",
            manifest: r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "tests", task = 'test "unterminated' }]
"#,
            expected_reference: "test \"unterminated",
            expected_detail: "unterminated quote",
        },
        ManagedTaskRefInvalidCase {
            workspace: "managed-process-task-ref-unterminated-quote",
            manifest: r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "tests", task = 'test "unterminated' }]
"#,
            expected_reference: "test \"unterminated",
            expected_detail: "unterminated quote",
        },
        ManagedTaskRefInvalidCase {
            workspace: "managed-process-task-ref-trailing-escape",
            manifest: r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "tests", task = "test vitest \\" }]
"#,
            expected_reference: "test vitest \\",
            expected_detail: "trailing escape",
        },
    ];

    for case in cases {
        let root = temp_workspace(case.workspace);
        write_manifest(&root.join("effigy.toml"), case.manifest);
        let err = run_dev_with_repo(&root, &[]).expect_err("invalid process task ref should fail");
        assert_managed_task_reference_invalid(
            err,
            "dev",
            "tests",
            case.expected_reference,
            case.expected_detail,
        );
    }
}

#[test]
fn run_manifest_task_managed_tui_supports_relative_task_refs() {
    let _guard = lock_test();
    let root = temp_workspace("managed-relative-task-ref");
    let dairy = create_workspace_dir(&root, "dairy");
    let froyo = root.join("froyo");
    let _env = managed_tui_env();

    write_manifest(
        &dairy.join("effigy.toml"),
        r#"[catalog]
alias = "dairy"
[tasks.dev]
mode = "tui"
concurrent = [{ name = "validate-stack", task = "../froyo/validate" }]
"#,
    );
    write_catalogs_with_tasks(
        &root,
        &[("froyo", &[("validate", "printf froyo-validate")])],
    );

    let out = run_task_with_repo(&root, "dairy/dev", &[]).expect("managed plan should render");
    assert_contains_all(
        &out,
        &[
            "validate-stack",
            "froyo-validate",
            &froyo.display().to_string(),
        ],
    );
}

#[test]
fn run_manifest_task_managed_tui_appends_shell_process_when_enabled() {
    let _guard = lock_test();
    let root = temp_workspace("managed-shell-enabled");
    let _env = managed_tui_env();
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
shell = true
concurrent = [{ name = "api", run = "printf api" }]
"#,
    );

    let out = run_dev(&root, &[]).expect("managed plan should include shell process");
    assert_contains_all(&out, &["shell", "exec ${SHELL:-/bin/zsh} -i"]);
}

#[test]
fn run_manifest_task_managed_tui_uses_global_shell_run_override() {
    let _guard = lock_test();
    let root = temp_workspace("managed-shell-global-override");
    let _env = managed_tui_env();
    write_root_manifest(
        &root,
        r#"[shell]
run = "exec ${SHELL:-/bin/bash} -i"

[tasks.dev]
mode = "tui"
shell = true
concurrent = [{ name = "api", run = "printf api" }]
"#,
    );

    let out = run_dev(&root, &[]).expect("managed plan should include configured shell process");
    assert_contains_all(&out, &["shell", "exec ${SHELL:-/bin/bash} -i"]);
}
