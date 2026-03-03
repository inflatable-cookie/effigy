use super::super::prelude::*;

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
