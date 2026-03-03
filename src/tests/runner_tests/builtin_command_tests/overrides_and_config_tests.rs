use super::prelude::*;

#[test]
fn run_manifest_task_builtin_test_plan_respects_configured_package_manager() {
    let root = temp_workspace("builtin-test-plan-package-manager");
    write_js_package_manager_manifest(&root, "pnpm");
    write_package_json_with_vitest_dev_dependency(&root);

    let out = run_builtin_ok(root, "test", &["--plan"]);
    assert_contains_all(&out, &["pnpm exec vitest run", "package_manager.js=pnpm"]);
}

#[test]
fn run_manifest_task_builtin_test_exec_uses_configured_package_manager() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-exec-package-manager");
    write_js_package_manager_manifest(&root, "bun");
    write_package_json_with_vitest_dev_dependency(&root);

    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    let bun_stub = bin_dir.join("bun");
    let args_log = root.join("bun-args.log");
    write_executable(
        &bun_stub,
        "#!/bin/sh\nprintf \"%s\\n\" \"$@\" > \"$EFFIGY_TEST_BUN_ARGS_FILE\"\n",
    );

    let prior_path = std::env::var("PATH").ok().unwrap_or_default();
    let path = format!("{}:{}", bin_dir.display(), prior_path);
    let _env = EnvGuard::set_many(&[
        ("PATH", Some(path)),
        ("SHELL", Some("/bin/sh".to_owned())),
        (
            "EFFIGY_TEST_BUN_ARGS_FILE",
            Some(args_log.display().to_string()),
        ),
    ]);

    let out = run_builtin_ok(root, "test", &["vitest"]);
    assert_contains_all(&out, &["Test Results"]);
    let args = fs::read_to_string(args_log).expect("read bun args");
    assert_eq!(args, "x\nvitest\nrun\n");
}

#[test]
fn run_manifest_task_builtin_test_plan_respects_runner_command_override() {
    let root = temp_workspace("builtin-test-plan-runner-override");
    write_root_manifest(
        &root,
        r#"[test.runners]
vitest = "pnpm exec vitest run --config vitest.config.ts"
"#,
    );
    write_package_json_with_vitest_dev_dependency(&root);

    let out = run_builtin_ok(root, "test", &["--plan", "vitest"]);
    assert_contains_all(
        &out,
        &[
            "pnpm exec vitest run --config vitest.config.ts",
            "test.runners.vitest command override applied",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_test_runner_override_wins_over_package_manager() {
    let root = temp_workspace("builtin-test-plan-override-precedence");
    write_root_manifest(
        &root,
        r#"[package_manager]
js = "bun"

[test.runners]
vitest = "npx vitest run --reporter=dot"
"#,
    );
    write_package_json_with_vitest_dev_dependency(&root);

    let out = run_builtin_ok(root, "test", &["--plan", "vitest"]);
    assert_contains_all(
        &out,
        &[
            "npx vitest run --reporter=dot",
            "package_manager.js=bun",
            "test.runners.vitest command override applied",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_config_prints_reference() {
    let root = temp_workspace("builtin-config");
    write_root_manifest(&root, "");

    let out = run_builtin_ok(root, "config", &[]);
    assert_contains_all(
        &out,
        &[
            "effigy.toml Reference",
            "cargo_env_match = \"prefix-aware\"",
            "[test.runners]",
            "[tasks]",
            "task = \"test vitest \\\"user service\\\"\"",
            "run = [{ id = \"tests\", task = \"test vitest \\\"user service\\\"\" }, { id = \"report\", run = \"printf validate-ok\", depends_on = [\"tests\"] }]",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_test_plan_has_blank_line_between_sections() {
    let root = temp_workspace("builtin-test-plan-section-spacing");
    write_package_json_with_vitest_dev_dependency(&root);

    let out = run_builtin_ok(root, "test", &["--plan"]);
    assert_contains_all(&out, &["\n\nTarget Summary\n", "\n\nTarget: root\n"]);
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

    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    let cargo_stub = bin_dir.join("cargo");
    let cargo_nextest_stub = bin_dir.join("cargo-nextest");
    let marker = root.join("cargo-env.log");
    write_executable(
        &cargo_stub,
        "#!/bin/sh\nprintf \"%s|%s\" \"$CARGO_HOME\" \"$CARGO_TARGET_DIR\" > \"$EFFIGY_TEST_CARGO_ENV_FILE\"\n",
    );
    write_executable(&cargo_nextest_stub, "#!/bin/sh\nexit 0\n");

    let prior_path = std::env::var("PATH").ok().unwrap_or_default();
    let path = format!("{}:{}", bin_dir.display(), prior_path);
    let _env = EnvGuard::set_many(&[
        ("PATH", Some(path)),
        (
            "EFFIGY_TEST_CARGO_ENV_FILE",
            Some(marker.display().to_string()),
        ),
    ]);

    let out = run_builtin_ok(root.clone(), "test", &[]);
    assert_contains_all(&out, &["Test Results", "root"]);

    let rendered = fs::read_to_string(&marker).expect("read cargo env marker");
    let parts = rendered.split('|').collect::<Vec<&str>>();
    assert_eq!(
        parts.len(),
        2,
        "expected cargo env marker format `home|target`"
    );
    assert!(parts[0].ends_with("/.cache/cargo/home"));
    assert!(parts[1].ends_with("/.cache/cargo/target"));
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

    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    let cargo_stub = bin_dir.join("cargo");
    let cargo_nextest_stub = bin_dir.join("cargo-nextest");
    let marker = root.join("cargo-env-direct.log");
    write_executable(
        &cargo_stub,
        "#!/bin/sh\nprintf \"%s|%s\" \"$CARGO_HOME\" \"$CARGO_TARGET_DIR\" > \"$EFFIGY_TEST_CARGO_ENV_FILE\"\n",
    );
    write_executable(&cargo_nextest_stub, "#!/bin/sh\nexit 0\n");

    let prior_path = std::env::var("PATH").ok().unwrap_or_default();
    let path = format!("{}:{}", bin_dir.display(), prior_path);
    let _env = EnvGuard::set_many(&[
        ("PATH", Some(path)),
        (
            "EFFIGY_TEST_CARGO_ENV_FILE",
            Some(marker.display().to_string()),
        ),
    ]);

    let out = run_builtin_ok(root.clone(), "test", &[]);
    assert_contains_all(&out, &["Test Results", "root"]);

    let rendered = fs::read_to_string(&marker).expect("read cargo env marker");
    let parts = rendered.split('|').collect::<Vec<&str>>();
    assert_eq!(
        parts.len(),
        2,
        "expected cargo env marker format `home|target`"
    );
    assert!(parts[0].ends_with("/.cache/cargo/direct-home"));
    assert!(parts[1].ends_with("/.cache/cargo/direct-target"));
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

    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    let cargo_stub = bin_dir.join("cargo");
    let marker = root.join("cargo-env-configured.log");
    write_executable(
        &cargo_stub,
        "#!/bin/sh\nprintf \"%s|%s\" \"$CARGO_HOME\" \"$CARGO_TARGET_DIR\" > \"$EFFIGY_TEST_CARGO_ENV_FILE\"\n",
    );

    let prior_path = std::env::var("PATH").ok().unwrap_or_default();
    let path = format!("{}:{}", bin_dir.display(), prior_path);
    let _env = EnvGuard::set_many(&[
        ("PATH", Some(path)),
        (
            "EFFIGY_TEST_CARGO_ENV_FILE",
            Some(marker.display().to_string()),
        ),
    ]);

    let out = run_builtin_ok(root.clone(), "test", &[]);
    assert_contains_all(&out, &["Test Results", "root"]);

    let rendered = fs::read_to_string(&marker).expect("read cargo env marker");
    let parts = rendered.split('|').collect::<Vec<&str>>();
    assert_eq!(
        parts.len(),
        2,
        "expected cargo env marker format `home|target`"
    );
    assert!(parts[0].ends_with("/.cache/cargo/configured-home"));
    assert!(parts[1].ends_with("/.cache/cargo/configured-target"));
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

    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    let cargo_stub = bin_dir.join("cargo");
    let marker = root.join("cargo-env-prefixed.log");
    write_executable(
        &cargo_stub,
        "#!/bin/sh\nprintf \"%s|%s\" \"$CARGO_HOME\" \"$CARGO_TARGET_DIR\" > \"$EFFIGY_TEST_CARGO_ENV_FILE\"\n",
    );

    let prior_path = std::env::var("PATH").ok().unwrap_or_default();
    let path = format!("{}:{}", bin_dir.display(), prior_path);
    let _env = EnvGuard::set_many(&[
        ("PATH", Some(path)),
        (
            "EFFIGY_TEST_CARGO_ENV_FILE",
            Some(marker.display().to_string()),
        ),
    ]);

    let out = run_builtin_ok(root.clone(), "test", &[]);
    assert_contains_all(&out, &["Test Results", "root"]);

    let rendered = fs::read_to_string(&marker).expect("read cargo env marker");
    let parts = rendered.split('|').collect::<Vec<&str>>();
    assert_eq!(
        parts.len(),
        2,
        "expected cargo env marker format `home|target`"
    );
    assert!(parts[0].ends_with("/.cache/cargo/prefixed-home"));
    assert!(parts[1].ends_with("/.cache/cargo/prefixed-target"));
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

    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    let nextest_stub = bin_dir.join("cargo-nextest");
    let marker = root.join("cargo-env-nextest.log");
    write_executable(
        &nextest_stub,
        "#!/bin/sh\nprintf \"%s|%s\" \"$CARGO_HOME\" \"$CARGO_TARGET_DIR\" > \"$EFFIGY_TEST_CARGO_ENV_FILE\"\n",
    );

    let prior_path = std::env::var("PATH").ok().unwrap_or_default();
    let path = format!("{}:{}", bin_dir.display(), prior_path);
    let _env = EnvGuard::set_many(&[
        ("PATH", Some(path)),
        (
            "EFFIGY_TEST_CARGO_ENV_FILE",
            Some(marker.display().to_string()),
        ),
    ]);

    let out = run_builtin_ok(root.clone(), "test", &[]);
    assert_contains_all(&out, &["Test Results", "root"]);

    let rendered = fs::read_to_string(&marker).expect("read cargo env marker");
    let parts = rendered.split('|').collect::<Vec<&str>>();
    assert_eq!(
        parts.len(),
        2,
        "expected cargo env marker format `home|target`"
    );
    assert!(parts[0].ends_with("/.cache/cargo/nextest-home"));
    assert!(parts[1].ends_with("/.cache/cargo/nextest-target"));
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

    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    let cargo_stub = bin_dir.join("cargo");
    let marker = root.join("cargo-env-shell-aware.log");
    write_executable(
        &cargo_stub,
        "#!/bin/sh\nprintf \"%s|%s\" \"$CARGO_HOME\" \"$CARGO_TARGET_DIR\" > \"$EFFIGY_TEST_CARGO_ENV_FILE\"\n",
    );

    let prior_path = std::env::var("PATH").ok().unwrap_or_default();
    let path = format!("{}:{}", bin_dir.display(), prior_path);
    let _env = EnvGuard::set_many(&[
        ("PATH", Some(path)),
        (
            "EFFIGY_TEST_CARGO_ENV_FILE",
            Some(marker.display().to_string()),
        ),
    ]);

    let out = run_builtin_ok(root.clone(), "test", &[]);
    assert_contains_all(&out, &["Test Results", "root"]);

    let rendered = fs::read_to_string(&marker).expect("read cargo env marker");
    let parts = rendered.split('|').collect::<Vec<&str>>();
    assert_eq!(
        parts.len(),
        2,
        "expected cargo env marker format `home|target`"
    );
    assert!(parts[0].ends_with("/.cache/cargo/shell-aware-home"));
    assert!(parts[1].ends_with("/.cache/cargo/shell-aware-target"));
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

    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    let cargo_stub = bin_dir.join("cargo");
    let marker = root.join("cargo-env-executable-only.log");
    write_executable(
        &cargo_stub,
        "#!/bin/sh\nprintf \"%s|%s\" \"$CARGO_HOME\" \"$CARGO_TARGET_DIR\" > \"$EFFIGY_TEST_CARGO_ENV_FILE\"\n",
    );

    let prior_path = std::env::var("PATH").ok().unwrap_or_default();
    let path = format!("{}:{}", bin_dir.display(), prior_path);
    let _env = EnvGuard::set_many(&[
        ("PATH", Some(path)),
        ("CARGO_HOME", None),
        ("CARGO_TARGET_DIR", None),
        (
            "EFFIGY_TEST_CARGO_ENV_FILE",
            Some(marker.display().to_string()),
        ),
    ]);

    let out = run_builtin_ok(root.clone(), "test", &[]);
    assert_contains_all(&out, &["Test Results", "root"]);

    let rendered = fs::read_to_string(&marker).expect("read cargo env marker");
    assert_eq!(rendered, "|");
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

    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    let cargo_stub = bin_dir.join("cargo");
    let marker = root.join("cargo-env-shell-wrapped.log");
    write_executable(
        &cargo_stub,
        "#!/bin/sh\nprintf \"%s|%s\" \"$CARGO_HOME\" \"$CARGO_TARGET_DIR\" > \"$EFFIGY_TEST_CARGO_ENV_FILE\"\n",
    );

    let prior_path = std::env::var("PATH").ok().unwrap_or_default();
    let path = format!("{}:{}", bin_dir.display(), prior_path);
    let _env = EnvGuard::set_many(&[
        ("PATH", Some(path)),
        ("CARGO_HOME", None),
        ("CARGO_TARGET_DIR", None),
        (
            "EFFIGY_TEST_CARGO_ENV_FILE",
            Some(marker.display().to_string()),
        ),
    ]);

    let out = run_builtin_ok(root.clone(), "test", &[]);
    assert_contains_all(&out, &["Test Results", "root"]);

    let rendered = fs::read_to_string(&marker).expect("read cargo env marker");
    assert_eq!(rendered, "|");
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

    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    let ct_stub = bin_dir.join("ct");
    let marker = root.join("cargo-env-non-cargo.log");
    write_executable(
        &ct_stub,
        "#!/bin/sh\nprintf \"%s|%s\" \"$CARGO_HOME\" \"$CARGO_TARGET_DIR\" > \"$EFFIGY_TEST_CARGO_ENV_FILE\"\n",
    );

    let prior_path = std::env::var("PATH").ok().unwrap_or_default();
    let path = format!("{}:{}", bin_dir.display(), prior_path);
    let _env = EnvGuard::set_many(&[
        ("PATH", Some(path)),
        ("CARGO_HOME", None),
        ("CARGO_TARGET_DIR", None),
        (
            "EFFIGY_TEST_CARGO_ENV_FILE",
            Some(marker.display().to_string()),
        ),
    ]);

    let out = run_builtin_ok(root.clone(), "test", &[]);
    assert_contains_all(&out, &["Test Results", "root"]);

    let rendered = fs::read_to_string(&marker).expect("read cargo env marker");
    assert_eq!(rendered, "|");
}
