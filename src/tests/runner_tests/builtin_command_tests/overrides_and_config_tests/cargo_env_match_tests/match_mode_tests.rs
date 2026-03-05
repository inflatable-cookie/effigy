use super::prelude::*;

#[test]
fn run_manifest_task_builtin_test_applies_manifest_cargo_env_for_shell_wrapped_command_when_configured(
) {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-cargo-env-shell-aware-positive");
    write_root_manifest(
        &root,
        r#"[env]
CARGO_HOME = "{project}/.cache/cargo/shell-aware-home"
CARGO_TARGET_DIR = "{project}/.cache/cargo/shell-aware-target"

[test]
cargo_env_match = "shell-aware"

[test.suites]
integration = "sh -lc 'cargo nextest run --workspace'"
"#,
    );

    let _env = setup_path_with_probes(
        &root,
        &[("cargo", CARGO_ENV_PROBE_SCRIPT)],
        &root.join("cargo-env.log"),
        false,
    );

    assert_cargo_env_applied(
        &root,
        "/.cache/cargo/shell-aware-home",
        "/.cache/cargo/shell-aware-target",
    );
}

#[test]
fn run_manifest_task_builtin_test_does_not_apply_manifest_cargo_env_for_prefixed_command_when_executable_only(
) {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-cargo-env-executable-only-negative");
    write_root_manifest(
        &root,
        r#"[env]
CARGO_HOME = "{project}/.cache/cargo/executable-only-home"
CARGO_TARGET_DIR = "{project}/.cache/cargo/executable-only-target"

[test]
cargo_env_match = "executable-only"

[test.suites]
integration = "env RUST_BACKTRACE=1 cargo nextest run --workspace"
"#,
    );

    let marker = root.join("cargo-env.log");
    let _env = setup_path_with_probes(&root, &[("cargo", CARGO_ENV_PROBE_SCRIPT)], &marker, true);

    let out = run_builtin_ok(root, "test", &[]);
    assert_output_contains_all(&out, &["Test Results", "root"]);
    assert_cargo_env_absent(&marker);
}

#[test]
fn run_manifest_task_builtin_test_does_not_apply_manifest_cargo_env_for_shell_wrapped_cargo_command(
) {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-cargo-env-shell-wrapped-negative");
    write_root_manifest(
        &root,
        r#"[env]
CARGO_HOME = "{project}/.cache/cargo/should-not-apply-home"
CARGO_TARGET_DIR = "{project}/.cache/cargo/should-not-apply-target"

[test.suites]
integration = "sh -lc 'cargo nextest run --workspace'"
"#,
    );

    let marker = root.join("cargo-env.log");
    let _env = setup_path_with_probes(&root, &[("cargo", CARGO_ENV_PROBE_SCRIPT)], &marker, true);

    let out = run_builtin_ok(root, "test", &[]);
    assert_output_contains_all(&out, &["Test Results", "root"]);
    assert_cargo_env_absent(&marker);
}

#[test]
fn run_manifest_task_builtin_test_does_not_apply_manifest_cargo_env_for_non_cargo_binary_command() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-cargo-env-non-cargo-negative");
    write_root_manifest(
        &root,
        r#"[env]
CARGO_HOME = "{project}/.cache/cargo/should-not-apply-home"
CARGO_TARGET_DIR = "{project}/.cache/cargo/should-not-apply-target"

[test.suites]
integration = "ct nextest run --workspace"
"#,
    );

    let marker = root.join("cargo-env.log");
    let _env = setup_path_with_probes(&root, &[("ct", CARGO_ENV_PROBE_SCRIPT)], &marker, true);

    let out = run_builtin_ok(root, "test", &[]);
    assert_output_contains_all(&out, &["Test Results", "root"]);
    assert_cargo_env_absent(&marker);
}
