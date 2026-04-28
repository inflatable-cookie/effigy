use crate::contract_test_support::{wait_for_path_exists, ExecutableOverrideGuard};
use crate::runner::tests::prelude::{
    assert_managed_non_zero_exit_case_table, assert_managed_output_case_table,
    assert_managed_profile_not_found_case_table, assert_managed_stream_builtin_test_case_table,
    assert_managed_stream_builtin_test_profile_entry, install_fake_container_runtime,
    install_fake_docker_ps_with_stale_project, lock_test, managed_stream_env,
    write_managed_stream_container_lifecycle_manifest, write_managed_stream_profile_manifest,
    write_root_manifest, EnvGuard, ManagedInvocation, ManagedNonZeroExitCase, ManagedOutputCase,
    ManagedProfileNotFoundCase, ManagedStreamBuiltinTestCase, Path,
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
    write_managed_stream_container_lifecycle_manifest(
        root,
        r#"[
  { role = "lifecycle", start = 1, tab = 1 },
  { name = "window", run = "true", start = 2, tab = 2, start_after_ms = 300, shutdown_on_exit = true }
]"#,
        "",
        "working_dir = \"/workspace\"\ncontainer = \"web\"",
        "demo-web-dev",
        "working_dir = \"/workspace\"",
        "",
    );
}

fn setup_managed_stream_lifecycle_workspace_binding(root: &Path) {
    write_managed_stream_container_lifecycle_manifest(
        root,
        r#"[
  { role = "lifecycle", start = 1, tab = 1 },
  { name = "window", run = "true", start = 2, tab = 2, start_after_ms = 300, shutdown_on_exit = true }
]"#,
        "",
        "working_dir = \"/workspace\"\ncontainer = \"web\"",
        "demo-web-dev",
        "",
        "",
    );
}

fn setup_managed_stream_lifecycle_inline_workspace_binding(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
workspace = "app"
container_lifecycle = true
concurrent = [
  { role = "lifecycle", start = 1, tab = 1 },
  { name = "window", run = "true", start = 2, tab = 2, start_after_ms = 300, shutdown_on_exit = true }
]

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
working_dir = "."
container = { image = "alpine:latest", mount = "./:/workspace" }
"#,
    );
}

fn setup_managed_stream_readiness(root: &Path) {
    write_managed_stream_container_lifecycle_manifest(
        root,
        r#"[
  { role = "lifecycle", start = 1, tab = 1 },
  { name = "window", run = "sh -lc 'sleep 1; exit 0'", start = 2, tab = 2, shutdown_on_exit = true }
]"#,
        "health_wait = true\nready_message = \"http://project.test\"",
        "working_dir = \"/workspace\"\ncontainer = \"web\"",
        "demo-web-dev",
        "working_dir = \"/workspace\"",
        "",
    );
}

fn setup_managed_stream_gateway(root: &Path) {
    write_managed_stream_container_lifecycle_manifest(
        root,
        r#"[
  { role = "lifecycle", start = 1, tab = 1 },
  { name = "window", run = "sh -lc 'sleep 1; exit 0'", start = 2, tab = 2, shutdown_on_exit = true }
]"#,
        "gateway = true\nhealth_wait = true\nready_message = \"http://project.test\"",
        "working_dir = \"/workspace\"\ncontainer = \"web\"",
        "demo-web-dev",
        "",
        "[containers.web.host]\nports = [\"8080:80\"]\n\n[containers.web.dns]\nroutes = [{ domain = \"project.test\" }]",
    );
}

fn setup_managed_stream_gateway_without_ready_message(root: &Path) {
    // Note: health_wait is intentionally false here. This test exercises
    // the ready-message + dns_routes banner derivation (which fires when
    // `ready_message` is unset); the readiness probe loop added in
    // commit 247198fb has its own unit-level coverage in
    // `managed_lifecycle_command_waits_for_probe_urls_before_ready`.
    // Enabling health_wait here would make the lifecycle script spin in
    // curl-against-fake-runtime forever and the `window` shutdown_on_exit
    // process would tear it down before the banner ever printed.
    write_managed_stream_container_lifecycle_manifest(
        root,
        r#"[
  { role = "lifecycle", start = 1, tab = 1 },
  { name = "window", run = "sh -lc 'sleep 1; exit 0'", start = 2, tab = 2, shutdown_on_exit = true }
]"#,
        "gateway = true\nhealth_wait = false",
        "working_dir = \"/workspace\"\ncontainer = \"web\"",
        "demo-web-dev",
        "",
        "[containers.web.host]\nports = [\"8080:80\"]\n\n[containers.web.dns]\nroutes = [\n  { domain = \"project.test\" },\n  { domain = \"admin.project.test\", tls = true }\n]",
    );
}

fn setup_managed_stream_container_routed_task_ref(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
workspace = "app"
container_lifecycle = true
concurrent = [
  { role = "lifecycle", start = 1, tab = 1 },
  { task = "api", start = 2, tab = 2 },
  { name = "window", run = "sh -lc 'sleep 1; exit 0'", start = 3, tab = 3, shutdown_on_exit = true }
]

[tasks.api]
run = "printf host-inline-api"

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "detached"
compose_file = "docker-compose.yml"
project_name = "demo-web-dev"
primary_service = "app"
working_dir = "/workspace"

[containers.web.host]
ports = ["8080:80"]
"#,
    );
    fs::write(
        root.join("docker-compose.yml"),
        "services:\n  app:\n    image: alpine:latest\n",
    )
    .expect("write docker compose");
}

fn setup_managed_stream_stale_project_name_mismatch(root: &Path) {
    write_managed_stream_container_lifecycle_manifest(
        root,
        r#"[
  { role = "lifecycle", start = 1, tab = 1 },
  { name = "window", run = "true", start = 2, tab = 2, shutdown_on_exit = true }
]"#,
        "",
        "container = \"web\"",
        "demo-web-renamed",
        "working_dir = \"/workspace\"",
        "",
    );
}

fn write_fake_effigy(root: &Path) -> std::path::PathBuf {
    let script = root.join("fake-effigy.sh");
    let log = root.join("fake-effigy.log");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\nlog='{}'\ncase \"$1\" in\n  container)\n    shift\n    name='default'\n    case \"$1\" in\n      up|down|status) ;;\n      *) name=\"$1\"; shift ;;\n    esac\n    sub=\"$1\"\n    shift\n    case \"$sub\" in\n      up)\n        printf 'up:%s\\n' \"$name\" >> \"$log\"\n        exit 0\n        ;;\n      status)\n        printf 'status:%s\\n' \"$name\" >> \"$log\"\n        printf 'fake-status-%s\\n' \"$name\"\n        exit 0\n        ;;\n      down)\n        printf 'down:%s\\n' \"$name\" >> \"$log\"\n        exit 0\n        ;;\n      shell)\n        printf 'shell:%s:%s\\n' \"$name\" \"$*\" >> \"$log\"\n        exit 0\n        ;;\n      *)\n        printf 'unexpected-container:%s %s\\n' \"$name\" \"$sub\" >> \"$log\"\n        exit 1\n        ;;\n    esac\n    ;;\n  gateway)\n    shift\n    case \"$1\" in\n      up)\n        printf 'gateway-up\\n' >> \"$log\"\n        exit 0\n        ;;\n      *)\n        printf 'unexpected-gateway:%s\\n' \"$*\" >> \"$log\"\n        exit 1\n        ;;\n    esac\n    ;;\n  exec)\n    shift\n    printf 'exec:%s\\n' \"$*\" >> \"$log\"\n    printf 'fake-exec\\n'\n    exit 0\n    ;;\n  *)\n    printf 'task:%s\\n' \"$*\" >> \"$log\"\n    printf 'fake-task-%s\\n' \"$1\"\n    exit 0\n    ;;\nesac\n",
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
    ];

    assert_managed_stream_builtin_test_case_table(&cases);
    assert_managed_stream_builtin_test_profile_entry(
        "managed-stream-builtin-test-profile-entry",
        "unit",
        "test",
    );
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
    let _runtime = install_fake_container_runtime(&root);
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
fn run_manifest_task_managed_stream_workspace_binding_owns_container_shutdown() {
    let _guard = lock_test();
    let _env = managed_stream_env();
    let root =
        crate::runner::tests::prelude::temp_workspace("managed-stream-lifecycle-workspace-binding");
    setup_managed_stream_lifecycle_workspace_binding(&root);
    let _runtime = install_fake_container_runtime(&root);
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
fn run_manifest_task_managed_stream_inline_workspace_binding_owns_container_shutdown() {
    let _guard = lock_test();
    let _env = managed_stream_env();
    let root = crate::runner::tests::prelude::temp_workspace(
        "managed-stream-lifecycle-inline-workspace-binding",
    );
    setup_managed_stream_lifecycle_inline_workspace_binding(&root);
    let _runtime = install_fake_container_runtime(&root);

    let out =
        crate::runner::tests::prelude::run_dev(&root, &[]).expect("managed run should succeed");
    assert!(
        out.contains("process `window` requested managed shutdown on exit"),
        "got: {out}"
    );

    let docker_log_path = root.join("fake-docker.log");
    wait_for_path_exists(&docker_log_path, Duration::from_secs(2), "fake docker log");
    let log = fs::read_to_string(&docker_log_path).expect("read fake docker log");
    assert!(log.contains("compose:ps"), "log: {log}");
    assert!(log.contains("compose:exec:workspace"), "log: {log}");
    assert!(log.contains("compose:down"), "log: {log}");
}

#[test]
fn run_manifest_task_managed_stream_projects_ready_message_from_lifecycle_owner() {
    let _guard = lock_test();
    let _env = managed_stream_env();
    let root = crate::runner::tests::prelude::temp_workspace("managed-stream-readiness");
    setup_managed_stream_readiness(&root);
    let _runtime = install_fake_container_runtime(&root);
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
    let _runtime = install_fake_container_runtime(&root);
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

#[test]
fn run_manifest_task_managed_stream_derives_ready_message_from_dns_routes() {
    let _guard = lock_test();
    let _env = managed_stream_env();
    let root =
        crate::runner::tests::prelude::temp_workspace("managed-stream-gateway-route-summary");
    setup_managed_stream_gateway_without_ready_message(&root);
    let _runtime = install_fake_container_runtime(&root);
    let fake_effigy = write_fake_effigy(&root);
    let _exec = ExecutableOverrideGuard::set(fake_effigy.display().to_string());

    let out =
        crate::runner::tests::prelude::run_dev(&root, &[]).expect("managed run should succeed");
    assert!(
        out.contains("managed ready: routes: http://project.test | https://admin.project.test"),
        "got: {out}"
    );
    assert!(out.contains("dns_routes:"), "got: {out}");
    assert!(out.contains("  - http://project.test -> app"), "got: {out}");
    assert!(
        out.contains("  - https://admin.project.test -> app"),
        "got: {out}"
    );
}

#[test]
fn run_manifest_task_managed_stream_handoff_skips_gateway_and_container_lifecycle_commands() {
    let _guard = lock_test();
    let _env = managed_stream_env();
    let _handoff =
        EnvGuard::set_many(&[("EFFIGY_INTERNAL_CONTAINER_HANDOFF", Some("1".to_owned()))]);
    let root =
        crate::runner::tests::prelude::temp_workspace("managed-stream-container-handoff-local");
    setup_managed_stream_gateway(&root);
    let _runtime = install_fake_container_runtime(&root);
    let fake_effigy = write_fake_effigy(&root);
    let _exec = ExecutableOverrideGuard::set(fake_effigy.display().to_string());

    let out =
        crate::runner::tests::prelude::run_dev(&root, &[]).expect("managed run should succeed");
    assert!(
        out.contains("process `window` requested managed shutdown on exit"),
        "got: {out}"
    );
    assert!(
        out.contains("managed lifecycle: workspace container is already running in handoff mode"),
        "got: {out}"
    );

    let log_path = root.join("fake-effigy.log");
    assert!(
        !log_path.exists(),
        "managed handoff should not invoke fake host effigy, but log exists: {}",
        log_path.display()
    );
}

#[test]
fn run_manifest_task_managed_stream_resolves_task_refs_before_container_exec_when_container_backed()
{
    let _guard = lock_test();
    let _env = managed_stream_env();
    let root =
        crate::runner::tests::prelude::temp_workspace("managed-stream-container-routed-task-ref");
    setup_managed_stream_container_routed_task_ref(&root);
    let _runtime = install_fake_container_runtime(&root);
    let fake_effigy = write_fake_effigy(&root);
    let _exec = ExecutableOverrideGuard::set(fake_effigy.display().to_string());

    let out =
        crate::runner::tests::prelude::run_dev(&root, &[]).expect("managed run should succeed");
    assert!(out.contains("summary  ok:3"), "got: {out}");

    let log_path = root.join("fake-effigy.log");
    wait_for_path_exists(&log_path, Duration::from_secs(2), "fake effigy log");
    let log = fs::read_to_string(&log_path).expect("read fake effigy log");
    assert!(log.contains("shell:web:"), "log: {log}");
    assert!(log.contains("--command true"), "log: {log}");
    assert!(log.contains("printf host-inline-api"), "log: {log}");
    assert!(!log.contains("task:api"), "log: {log}");
}

#[test]
fn run_manifest_task_managed_stream_fails_fast_for_stale_project_name_runtime() {
    let _guard = lock_test();
    let _env = managed_stream_env();
    let root =
        crate::runner::tests::prelude::temp_workspace("managed-stream-stale-project-name-runtime");
    setup_managed_stream_stale_project_name_mismatch(&root);
    let _runtime = install_fake_docker_ps_with_stale_project(&root, "demo-web-old");

    let error = crate::runner::tests::prelude::run_dev(&root, &[])
        .expect_err("managed run should fail for stale project name");
    let rendered = error.to_string();
    assert!(
        rendered.contains("expects Compose project `demo-web-renamed`"),
        "got: {rendered}"
    );
    assert!(rendered.contains("under `demo-web-old`"), "got: {rendered}");
    assert!(
        rendered.contains("project_name` changed while the old runtime was still up"),
        "got: {rendered}"
    );
}
