use super::prelude::*;

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
        &[
            ("cargo", CARGO_ENV_PROBE_SCRIPT),
            ("cargo-nextest", "#!/bin/sh\nexit 0\n"),
        ],
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
        &[
            ("cargo", CARGO_ENV_PROBE_SCRIPT),
            ("cargo-nextest", "#!/bin/sh\nexit 0\n"),
        ],
        &root.join("cargo-env.log"),
        false,
    );

    assert_cargo_env_applied(
        &root,
        "/.cache/cargo/direct-home",
        "/.cache/cargo/direct-target",
    );
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

    assert_cargo_env_applied(
        &root,
        "/.cache/cargo/nextest-home",
        "/.cache/cargo/nextest-target",
    );
}
