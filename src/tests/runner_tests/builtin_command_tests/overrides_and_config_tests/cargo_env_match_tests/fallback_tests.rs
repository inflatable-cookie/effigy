use super::prelude::{
    assert_cargo_env_matches, assert_output_contains_all, fs, lock_test, run_builtin_ok,
    setup_path_with_probes, temp_workspace, write_root_manifest, CARGO_ENV_PROBE_SCRIPT, EnvGuard,
};

#[test]
fn run_manifest_task_builtin_test_applies_process_env_fallback_for_missing_cargo_env() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-cargo-env-process-fallback");
    write_root_manifest(
        &root,
        r#"[test.suites]
integration = "cargo nextest run --workspace"
"#,
    );

    let marker = root.join("cargo-env.log");
    let _path = setup_path_with_probes(&root, &[("cargo", CARGO_ENV_PROBE_SCRIPT)], &marker, true);
    let _process = EnvGuard::set_many(&[
        ("CARGO_HOME", Some("/tmp/effigy-process-home".to_owned())),
        (
            "CARGO_TARGET_DIR",
            Some("/tmp/effigy-process-target".to_owned()),
        ),
    ]);

    let out = run_builtin_ok(root, "test", &[]);
    assert_output_contains_all(&out, &["Test Results", "root"]);
    assert_cargo_env_matches(
        &marker,
        "/tmp/effigy-process-home",
        "/tmp/effigy-process-target",
    );
}

#[test]
fn run_manifest_task_builtin_test_applies_dotenv_fallback_for_missing_cargo_env() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-cargo-env-dotenv-fallback");
    write_root_manifest(
        &root,
        r#"[test.suites]
integration = "cargo nextest run --workspace"
"#,
    );
    fs::write(
        root.join(".env"),
        "CARGO_HOME=/tmp/effigy-dotenv-home\nCARGO_TARGET_DIR=/tmp/effigy-dotenv-target\n",
    )
    .expect("write .env");

    let marker = root.join("cargo-env.log");
    let _path = setup_path_with_probes(&root, &[("cargo", CARGO_ENV_PROBE_SCRIPT)], &marker, true);

    let out = run_builtin_ok(root, "test", &[]);
    assert_output_contains_all(&out, &["Test Results", "root"]);
    assert_cargo_env_matches(
        &marker,
        "/tmp/effigy-dotenv-home",
        "/tmp/effigy-dotenv-target",
    );
}

#[test]
fn run_manifest_task_builtin_test_cargo_env_fallback_precedence_is_manifest_then_process_then_dotenv(
) {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-cargo-env-fallback-precedence");
    write_root_manifest(
        &root,
        r#"[env]
CARGO_HOME = "{project}/.cache/cargo/manifest-home"

[test.suites]
integration = "cargo nextest run --workspace"
"#,
    );
    fs::write(
        root.join(".env"),
        "CARGO_HOME=/tmp/effigy-dotenv-home\nCARGO_TARGET_DIR=/tmp/effigy-dotenv-target\n",
    )
    .expect("write .env");

    let marker = root.join("cargo-env.log");
    let _path = setup_path_with_probes(&root, &[("cargo", CARGO_ENV_PROBE_SCRIPT)], &marker, true);
    let _process = EnvGuard::set_many(&[
        ("CARGO_HOME", Some("/tmp/effigy-process-home".to_owned())),
        (
            "CARGO_TARGET_DIR",
            Some("/tmp/effigy-process-target".to_owned()),
        ),
    ]);

    let out = run_builtin_ok(root, "test", &[]);
    assert_output_contains_all(&out, &["Test Results", "root"]);
    assert_cargo_env_matches(
        &marker,
        "/.cache/cargo/manifest-home",
        "/tmp/effigy-process-target",
    );
}
