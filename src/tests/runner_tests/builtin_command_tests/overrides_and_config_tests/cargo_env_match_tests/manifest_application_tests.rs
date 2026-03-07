use super::prelude::{
    assert_cargo_env_applied, fs, lock_test, read_file_text, run_builtin_ok,
    setup_path_with_probes, temp_workspace, write_executable, write_multi_suite_cargo_manifest,
    write_root_manifest, EnvGuard, CARGO_ENV_PROBE_SCRIPT,
};

const SUITE_ENV_AND_CARGO_PROBE_SCRIPT: &str = "#!/bin/sh\nprintf \"%s|%s|%s\" \"$CARGO_HOME\" \"$CARGO_TARGET_DIR\" \"$TEST_DATABASE_URL\" > \"$EFFIGY_TEST_CARGO_ENV_FILE\"\n";
const SUITE_ENV_ONLY_PROBE_SCRIPT: &str =
    "#!/bin/sh\nprintf \"%s|%s\" \"$TEST_DATABASE_URL\" \"$DATABASE_URL\" > \"$EFFIGY_TEST_CARGO_ENV_FILE\"\n";

fn assert_suite_env_and_cargo_match(
    marker: &std::path::Path,
    expected_home: &str,
    expected_target: &str,
    expected_database_url: &str,
) {
    let rendered = read_file_text(marker);
    let parts = rendered.split('|').collect::<Vec<&str>>();
    assert_eq!(
        parts.len(),
        3,
        "expected suite env marker format `cargo_home|cargo_target|test_database_url`"
    );
    assert!(parts[0].ends_with(expected_home));
    assert!(parts[1].ends_with(expected_target));
    assert!(parts[2].ends_with(expected_database_url));
}

fn assert_suite_env_only_matches(
    marker: &std::path::Path,
    expected_test_database_url: &str,
    expected_database_url: &str,
) {
    let rendered = read_file_text(marker);
    let parts = rendered.split('|').collect::<Vec<&str>>();
    assert_eq!(
        parts.len(),
        2,
        "expected suite env marker format `test_database_url|database_url`"
    );
    assert!(parts[0].ends_with(expected_test_database_url));
    assert_eq!(parts[1], expected_database_url);
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

#[test]
fn run_manifest_task_builtin_test_layers_configured_suite_env_with_manifest_cargo_env() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-cargo-and-suite-env-configured-suite");
    write_root_manifest(
        &root,
        r#"[env]
CARGO_HOME = "{project}/.cache/cargo/suite-home"
CARGO_TARGET_DIR = "{project}/.cache/cargo/suite-target"

[test.suites.integration]
run = "cargo nextest run --workspace"
env = { TEST_DATABASE_URL = "{project}/db/test.sqlite" }
"#,
    );
    write_multi_suite_cargo_manifest(&root);

    let _env = setup_path_with_probes(
        &root,
        &[("cargo", SUITE_ENV_AND_CARGO_PROBE_SCRIPT)],
        &root.join("cargo-env.log"),
        false,
    );

    let out = run_builtin_ok(root.to_path_buf(), "test", &[]);
    assert!(out.contains("Test Results"));
    assert_suite_env_and_cargo_match(
        &root.join("cargo-env.log"),
        "/.cache/cargo/suite-home",
        "/.cache/cargo/suite-target",
        "/db/test.sqlite",
    );
}

#[test]
fn run_manifest_task_builtin_test_resolves_configured_suite_env_from_env_file() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-suite-env-file-configured-suite");
    write_root_manifest(
        &root,
        r#"[test.suites.managed]
run = "suite-probe"
env = "TEST_DATABASE_URL"
env_file = ".env.test"
"#,
    );
    fs::write(
        root.join(".env.test"),
        "TEST_DATABASE_URL={project}/db/from-dotenv.sqlite\n",
    )
    .expect("write env file");
    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    write_executable(&bin_dir.join("suite-probe"), SUITE_ENV_ONLY_PROBE_SCRIPT);
    let prior_path = std::env::var("PATH").ok().unwrap_or_default();
    let _env = EnvGuard::set_many(&[
        ("PATH", Some(format!("{}:{prior_path}", bin_dir.display()))),
        (
            "EFFIGY_TEST_CARGO_ENV_FILE",
            Some(root.join("suite-env.log").display().to_string()),
        ),
        (
            "DATABASE_URL",
            Some("postgres://process-database-url".to_owned()),
        ),
    ]);

    let out = run_builtin_ok(root.to_path_buf(), "test", &[]);
    assert!(out.contains("Test Results"));
    assert_suite_env_only_matches(
        &root.join("suite-env.log"),
        "/db/from-dotenv.sqlite",
        "postgres://process-database-url",
    );
}
