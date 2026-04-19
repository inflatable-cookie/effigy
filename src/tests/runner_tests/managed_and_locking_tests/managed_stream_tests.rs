use crate::contract_test_support::{wait_for_path_exists, ExecutableOverrideGuard};
use crate::runner::tests::prelude::{
    assert_managed_non_zero_exit_case_table, assert_managed_output_case_table,
    assert_managed_profile_not_found_case_table, assert_managed_stream_builtin_test_case_table,
    lock_test, managed_stream_env, write_managed_stream_profile_manifest, write_root_manifest,
    ManagedInvocation, ManagedNonZeroExitCase, ManagedOutputCase, ManagedProfileNotFoundCase,
    ManagedStreamBuiltinTestCase, Path,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

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

fn setup_managed_stream_shutdown_on_exit(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "window", run = "sh -lc 'exit 0'", shutdown_on_exit = true },
  { name = "watch", run = "sh -lc 'sleep 5; printf watch-still-running'" }
]
"#,
    );
}

fn setup_managed_stream_lifecycle(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
container_session = "web"
concurrent = [
  { role = "lifecycle", start = 1, tab = 1 },
  { name = "window", run = "sh -lc 'sleep 1; exit 0'", start = 2, tab = 2, start_after_ms = 300, shutdown_on_exit = true }
]

[tasks.dev.managed]
container_lifecycle = true
"#,
    );
}

fn setup_managed_stream_readiness(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
container_session = "web"
concurrent = [
  { role = "lifecycle", start = 1, tab = 1 },
  { name = "window", run = "sh -lc 'sleep 1; exit 0'", start = 2, tab = 2, shutdown_on_exit = true }
]

[tasks.dev.managed]
container_lifecycle = true
health_wait = true
ready_message = "http://project.test"
"#,
    );
}

fn setup_managed_stream_gateway(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
container_session = "web"
concurrent = [
  { role = "lifecycle", start = 1, tab = 1 },
  { name = "window", run = "sh -lc 'sleep 1; exit 0'", start = 2, tab = 2, shutdown_on_exit = true }
]

[tasks.dev.managed]
container_lifecycle = true
gateway = true
health_wait = true
ready_message = "http://project.test"

[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "detached"
compose_file = "docker-compose.yml"
project_name = "demo-web-dev"
primary_service = "app"

[containers.web.host]
ports = ["8080:80"]

[containers.web.dns]
domain = "project.test"
"#,
    );
    fs::write(
        root.join("docker-compose.yml"),
        "services:\n  app:\n    image: alpine:latest\n",
    )
    .expect("write docker compose");
}

fn write_fake_effigy(root: &Path) -> std::path::PathBuf {
    let script = root.join("fake-effigy.sh");
    let log = root.join("fake-effigy.log");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\nlog='{}'\ncase \"$1\" in\n  container)\n    shift\n    name='default'\n    case \"$1\" in\n      up|down|status) ;;\n      *) name=\"$1\"; shift ;;\n    esac\n    sub=\"$1\"\n    shift\n    case \"$sub\" in\n      up)\n        printf 'up:%s\\n' \"$name\" >> \"$log\"\n        exit 0\n        ;;\n      status)\n        printf 'status:%s\\n' \"$name\" >> \"$log\"\n        printf 'fake-status-%s\\n' \"$name\"\n        exit 0\n        ;;\n      down)\n        printf 'down:%s\\n' \"$name\" >> \"$log\"\n        exit 0\n        ;;\n      shell)\n        printf 'shell:%s\\n' \"$name\" >> \"$log\"\n        exit 0\n        ;;\n      *)\n        printf 'unexpected-container:%s %s\\n' \"$name\" \"$sub\" >> \"$log\"\n        exit 1\n        ;;\n    esac\n    ;;\n  gateway)\n    shift\n    case \"$1\" in\n      up)\n        printf 'gateway-up\\n' >> \"$log\"\n        exit 0\n        ;;\n      *)\n        printf 'unexpected-gateway:%s\\n' \"$*\" >> \"$log\"\n        exit 1\n        ;;\n    esac\n    ;;\n  *)\n    printf 'unexpected:%s\\n' \"$*\" >> \"$log\"\n    exit 1\n    ;;\nesac\n",
            log.display()
        ),
    )
    .expect("write fake effigy");
    let mut perms = fs::metadata(&script)
        .expect("stat fake effigy")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script, perms).expect("chmod fake effigy");
    script
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
        ManagedOutputCase {
            workspace: "managed-stream-shutdown-on-exit",
            invocation: ManagedInvocation::Dev,
            args: &[],
            expected: &[
                "Managed Task Runtime",
                "shutdown-on-exit: window",
                "process `window` requested managed shutdown on exit",
                "process `window` exit=0",
            ],
            expected_absent: &["watch-still-running"],
            setup: setup_managed_stream_shutdown_on_exit,
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

#[test]
fn run_manifest_task_managed_stream_lifecycle_process_owns_container_shutdown() {
    let _guard = lock_test();
    let _env = managed_stream_env();
    let root = crate::runner::tests::prelude::temp_workspace("managed-stream-lifecycle");
    setup_managed_stream_lifecycle(&root);
    let fake_effigy = write_fake_effigy(&root);
    let _exec = ExecutableOverrideGuard::set(fake_effigy.display().to_string());

    let out =
        crate::runner::tests::prelude::run_dev(&root, &[]).expect("managed run should succeed");
    assert!(
        out.contains("process `window` requested managed shutdown on exit"),
        "got: {out}"
    );

    let log_path = root.join("fake-effigy.log");
    wait_for_path_exists(&log_path, Duration::from_secs(2), "fake effigy log");
    let log = fs::read_to_string(&log_path).expect("read fake effigy log");
    assert!(log.contains("down:web"), "log: {log}");
}

#[test]
fn run_manifest_task_managed_stream_projects_ready_message_from_lifecycle_owner() {
    let _guard = lock_test();
    let _env = managed_stream_env();
    let root = crate::runner::tests::prelude::temp_workspace("managed-stream-readiness");
    setup_managed_stream_readiness(&root);
    let fake_effigy = write_fake_effigy(&root);
    let _exec = ExecutableOverrideGuard::set(fake_effigy.display().to_string());

    let out =
        crate::runner::tests::prelude::run_dev(&root, &[]).expect("managed run should succeed");
    assert!(out.contains("readiness-wait: enabled"), "got: {out}");
    assert!(
        out.contains("ready-message: http://project.test"),
        "got: {out}"
    );
}

#[test]
fn run_manifest_task_managed_stream_auto_starts_gateway_before_runtime() {
    let _guard = lock_test();
    let _env = managed_stream_env();
    let root = crate::runner::tests::prelude::temp_workspace("managed-stream-gateway");
    setup_managed_stream_gateway(&root);
    let fake_effigy = write_fake_effigy(&root);
    let _exec = ExecutableOverrideGuard::set(fake_effigy.display().to_string());

    let out =
        crate::runner::tests::prelude::run_dev(&root, &[]).expect("managed run should succeed");
    assert!(out.contains("gateway-auto-start: enabled"), "got: {out}");

    let log_path = root.join("fake-effigy.log");
    wait_for_path_exists(&log_path, Duration::from_secs(2), "fake effigy log");
    let log = fs::read_to_string(&log_path).expect("read fake effigy log");
    assert!(log.contains("gateway-up"), "log: {log}");
    assert!(log.contains("up:web"), "log: {log}");
}
