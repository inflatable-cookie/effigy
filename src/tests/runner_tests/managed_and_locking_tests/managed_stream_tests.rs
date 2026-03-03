use super::prelude::*;

#[test]
fn run_manifest_task_managed_stream_executes_selected_profile_processes() {
    let _guard = lock_test();
    let root = temp_workspace("managed-stream-runtime");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "printf api-ok" },
  { name = "front", run = "printf front-ok" }
]
"#,
    );
    let _env = managed_stream_env();

    let out = run_dev(&root, &[]).expect("managed stream run");
    assert_contains_all(
        &out,
        &[
            "Managed Task Runtime",
            "[api] api-ok",
            "[front] front-ok",
            "fail-on-non-zero: enabled",
            "process `api` exit=0",
            "process `front` exit=0",
        ],
    );
}

#[test]
fn run_manifest_task_managed_stream_uses_named_profile_concurrent_entries() {
    let _guard = lock_test();
    let root = temp_workspace("managed-stream-runtime-profile-specific");
    write_managed_stream_profile_manifest(&root);
    let _env = managed_stream_env();

    let out = run_dev(&root, &["front"]).expect("managed stream run with named profile");
    assert_contains_all(
        &out,
        &[
            "Managed Task Runtime",
            "profile: front",
            "[front-only] front-ok",
            "process `front-only` exit=0",
        ],
    );
    assert!(!out.contains("default-only"));
    assert!(!out.contains("default-ok"));
}

#[test]
fn run_manifest_task_managed_stream_errors_for_unknown_profile_with_available_profiles() {
    let _guard = lock_test();
    let root = temp_workspace("managed-stream-unknown-profile");
    write_managed_stream_profile_manifest(&root);
    let _env = managed_stream_env();

    let err = run_dev(&root, &["admin"]).expect_err("unknown managed profile should fail");
    assert_managed_profile_not_found(err, "dev", "admin", &["default", "front"]);
}

#[test]
fn run_manifest_task_managed_stream_profile_entry_supports_builtin_test() {
    let _guard = lock_test();
    let _env = managed_stream_env();
    let cases = [
        ManagedStreamBuiltinTestCase {
            workspace: "managed-stream-builtin-test-task-ref",
            suite: "unit",
            task_ref: "test",
        },
        ManagedStreamBuiltinTestCase {
            workspace: "managed-stream-builtin-test-task-ref-inline-suite",
            suite: "vitest",
            task_ref: "test vitest",
        },
        ManagedStreamBuiltinTestCase {
            workspace: "managed-stream-builtin-test-profile-entry",
            suite: "unit",
            task_ref: "test",
        },
    ];

    for case in cases {
        let root = temp_workspace(case.workspace);
        let marker = root.join("builtin-test-called.log");
        write_managed_stream_builtin_test_manifest(&root, case.suite, case.task_ref, &marker);

        let out =
            run_dev(&root, &["default"]).expect("run managed stream with builtin profile entry");
        assert_contains_all(&out, &["Managed Task Runtime", "root: ok"]);
        assert!(marker.exists(), "built-in test task ref should execute");
    }
}

#[test]
fn run_manifest_task_managed_stream_fails_when_process_exits_non_zero_by_default() {
    let _guard = lock_test();
    let root = temp_workspace("managed-stream-fail-on-non-zero-default");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "api", run = "sh -lc 'exit 7'" }]
"#,
    );
    let _env = managed_stream_env();

    let err =
        run_dev(&root, &[]).expect_err("managed stream should fail for non-zero exit by default");
    assert_managed_non_zero_exit(err, "dev", "default", &[("api", "exit=7")]);
}

#[test]
fn run_manifest_task_managed_stream_allows_non_zero_when_disabled() {
    let _guard = lock_test();
    let root = temp_workspace("managed-stream-fail-on-non-zero-disabled");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
fail_on_non_zero = false
concurrent = [{ name = "api", run = "sh -lc 'exit 9'" }]
"#,
    );
    let _env = managed_stream_env();

    let out = run_dev(&root, &[]).expect("managed stream should allow non-zero when disabled");
    assert_contains_all(
        &out,
        &[
            "Managed Task Runtime",
            "fail-on-non-zero: disabled",
            "process `api` exit=9",
        ],
    );
}
