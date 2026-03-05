use super::prelude::*;

fn setup_managed_stream_runtime(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "printf api-ok" },
  { name = "front", run = "printf front-ok" }
]
"#,
    );
}

fn setup_managed_stream_profile_manifest(root: &Path) {
    write_managed_stream_profile_manifest(root);
}

fn setup_managed_stream_fail_on_non_zero_default(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "api", run = "sh -lc 'exit 7'" }]
"#,
    );
}

fn setup_managed_stream_fail_on_non_zero_disabled(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
fail_on_non_zero = false
concurrent = [{ name = "api", run = "sh -lc 'exit 9'" }]
"#,
    );
}

#[test]
fn run_manifest_task_managed_stream_output_contract_table() {
    let _guard = lock_test();
    let _env = managed_stream_env();
    let cases = [
        ManagedOutputCase {
            workspace: "managed-stream-runtime",
            invocation: ManagedInvocation::Dev,
            args: &[],
            expected: &[
                "Managed Task Runtime",
                "[api] api-ok",
                "[front] front-ok",
                "fail-on-non-zero: enabled",
                "process `api` exit=0",
                "process `front` exit=0",
            ],
            expected_absent: &[],
            setup: setup_managed_stream_runtime,
        },
        ManagedOutputCase {
            workspace: "managed-stream-runtime-profile-specific",
            invocation: ManagedInvocation::Dev,
            args: &["front"],
            expected: &[
                "Managed Task Runtime",
                "profile: front",
                "[front-only] front-ok",
                "process `front-only` exit=0",
            ],
            expected_absent: &["default-only", "default-ok"],
            setup: setup_managed_stream_profile_manifest,
        },
        ManagedOutputCase {
            workspace: "managed-stream-fail-on-non-zero-disabled",
            invocation: ManagedInvocation::Dev,
            args: &[],
            expected: &[
                "Managed Task Runtime",
                "fail-on-non-zero: disabled",
                "process `api` exit=9",
            ],
            expected_absent: &[],
            setup: setup_managed_stream_fail_on_non_zero_disabled,
        },
    ];

    assert_managed_output_case_table(&cases);
}

#[test]
fn run_manifest_task_managed_stream_errors_for_unknown_profile_with_available_profiles() {
    let _guard = lock_test();
    let _env = managed_stream_env();
    let cases = [ManagedProfileNotFoundCase {
        workspace: "managed-stream-unknown-profile",
        invocation: ManagedInvocation::Dev,
        args: &["admin"],
        setup: setup_managed_stream_profile_manifest,
        expected_task: "dev",
        expected_profile: "admin",
        expected_available: &["default", "front"],
    }];

    assert_managed_profile_not_found_case_table(&cases);
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

    assert_managed_stream_builtin_test_case_table(&cases);
}

#[test]
fn run_manifest_task_managed_stream_fails_when_process_exits_non_zero_by_default() {
    let _guard = lock_test();
    let _env = managed_stream_env();
    let cases = [ManagedNonZeroExitCase {
        workspace: "managed-stream-fail-on-non-zero-default",
        invocation: ManagedInvocation::Dev,
        args: &[],
        setup: setup_managed_stream_fail_on_non_zero_default,
        expected_task: "dev",
        expected_profile: "default",
        expected_processes: &[("api", "exit=7")],
    }];

    assert_managed_non_zero_exit_case_table(&cases);
}
