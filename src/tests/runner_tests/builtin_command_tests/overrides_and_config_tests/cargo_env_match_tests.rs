use super::prelude::*;

const CARGO_ENV_PROBE_SCRIPT: &str =
    "#!/bin/sh\nprintf \"%s|%s\" \"$CARGO_HOME\" \"$CARGO_TARGET_DIR\" > \"$EFFIGY_TEST_CARGO_ENV_FILE\"\n";

fn assert_cargo_env_applied(root: &PathBuf, expected_home: &str, expected_target: &str) {
    let out = run_builtin_ok(root.clone(), "test", &[]);
    assert_contains_all(&out, &["Test Results", "root"]);
    assert_cargo_env_matches(&root.join("cargo-env.log"), expected_home, expected_target);
}

#[test]
fn run_manifest_task_builtin_test_applies_grouped_manifest_cargo_env() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-cargo-env-grouped");
    write_root_manifest(
        &root,
        r#"[env]
cargo = [
  { CARGO_HOME = "{project}/.cache/cargo/home" },
  { CARGO_TARGET_DIR = "{repo}/.cache/cargo/target" }
]
"#,
    );
    write_multi_suite_cargo_manifest(&root);

    let _env = setup_path_with_probes(
        &root,
        &[("cargo", CARGO_ENV_PROBE_SCRIPT), ("cargo-nextest", "#!/bin/sh\nexit 0\n")],
        &root.join("cargo-env.log"),
        false,
    );

    assert_cargo_env_applied(&root, "/.cache/cargo/home", "/.cache/cargo/target");
}

#[test]
fn run_manifest_task_builtin_test_prefers_direct_manifest_cargo_env_over_grouped_entries() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-cargo-env-direct-precedence");
    write_root_manifest(
        &root,
        r#"[env]
CARGO_HOME = "{project}/.cache/cargo/direct-home"
CARGO_TARGET_DIR = "{project}/.cache/cargo/direct-target"
cargo = [
  { CARGO_HOME = "{project}/.cache/cargo/profile-home" },
  { CARGO_TARGET_DIR = "{project}/.cache/cargo/profile-target" }
]
"#,
    );
    write_multi_suite_cargo_manifest(&root);

    let _env = setup_path_with_probes(
        &root,
        &[("cargo", CARGO_ENV_PROBE_SCRIPT), ("cargo-nextest", "#!/bin/sh\nexit 0\n")],
        &root.join("cargo-env.log"),
        false,
    );

    assert_cargo_env_applied(&root, "/.cache/cargo/direct-home", "/.cache/cargo/direct-target");
}

#[test]
fn run_manifest_task_builtin_test_applies_manifest_cargo_env_for_configured_cargo_suite() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-cargo-env-configured-suite");
    write_root_manifest(
        &root,
        r#"[env]
CARGO_HOME = "{project}/.cache/cargo/configured-home"
CARGO_TARGET_DIR = "{project}/.cache/cargo/configured-target"

[test.suites]
integration = "cargo nextest run --workspace"
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
        "/.cache/cargo/configured-home",
        "/.cache/cargo/configured-target",
    );
}

#[test]
fn run_manifest_task_builtin_test_applies_manifest_cargo_env_for_prefixed_cargo_command() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-cargo-env-prefixed-command");
    write_root_manifest(
        &root,
        r#"[env]
CARGO_HOME = "{project}/.cache/cargo/prefixed-home"
CARGO_TARGET_DIR = "{project}/.cache/cargo/prefixed-target"

[test.suites]
integration = "env RUST_BACKTRACE=1 cargo nextest run --workspace"
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
        "/.cache/cargo/prefixed-home",
        "/.cache/cargo/prefixed-target",
    );
}

#[test]
fn run_manifest_task_builtin_test_applies_manifest_cargo_env_for_cargo_nextest_binary_command() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-cargo-env-cargo-nextest-bin");
    write_root_manifest(
        &root,
        r#"[env]
CARGO_HOME = "{project}/.cache/cargo/nextest-home"
CARGO_TARGET_DIR = "{project}/.cache/cargo/nextest-target"

[test.suites]
integration = "cargo-nextest run --workspace"
"#,
    );

    let _env = setup_path_with_probes(
        &root,
        &[("cargo-nextest", CARGO_ENV_PROBE_SCRIPT)],
        &root.join("cargo-env.log"),
        false,
    );

    assert_cargo_env_applied(&root, "/.cache/cargo/nextest-home", "/.cache/cargo/nextest-target");
}

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
    let _env = setup_path_with_probes(
        &root,
        &[("cargo", CARGO_ENV_PROBE_SCRIPT)],
        &marker,
        true,
    );

    let out = run_builtin_ok(root, "test", &[]);
    assert_contains_all(&out, &["Test Results", "root"]);
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
    let _env = setup_path_with_probes(
        &root,
        &[("cargo", CARGO_ENV_PROBE_SCRIPT)],
        &marker,
        true,
    );

    let out = run_builtin_ok(root, "test", &[]);
    assert_contains_all(&out, &["Test Results", "root"]);
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
    assert_contains_all(&out, &["Test Results", "root"]);
    assert_cargo_env_absent(&marker);
}
