use crate::runner::tests::prelude::{
    assert_output_contains_all, assert_output_excludes_all, assert_path_exists,
    assert_path_missing, fs, install_local_vitest, install_local_vitest_marker, run_builtin_ok,
    temp_workspace, write_package_json_with_vitest_dev_dependency, write_root_manifest,
};

#[test]
fn run_manifest_task_builtin_test_skips_vitest_when_only_a_transitive_binary_exists() {
    let root = temp_workspace("builtin-test-skip-transitive-vitest");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
    )
    .expect("write cargo toml");
    fs::write(root.join("package.json"), r#"{ "name": "app" }"#).expect("write package");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("src/lib.rs"),
        "pub fn ok() -> bool { true }\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn smoke() {\n        assert!(super::ok());\n    }\n}\n",
    )
    .expect("write lib");
    let marker = root.join("vitest-called.log");
    install_local_vitest_marker(&root, &marker);

    let plan = run_builtin_ok(root.to_path_buf(), "test", &["--plan"]);
    assert_output_contains_all(
        &plan,
        &[
            "vitest skipped:",
            "installed `node_modules/.bin/vitest` is not package intent",
        ],
    );
    assert!(
        plan.contains("cargo-test") || plan.contains("cargo-nextest"),
        "expected a Rust suite when Vitest is skipped, got {plan}"
    );
    assert_output_excludes_all(&plan, &["available-suites: vitest"]);

    let out = run_builtin_ok(root, "test", &[]);
    assert_path_missing(&marker, "transitive vitest stub");
    assert_output_excludes_all(&out, &["root/vitest"]);
}

#[test]
fn run_manifest_task_builtin_test_runs_configured_vitest_test_dir() {
    let root = temp_workspace("builtin-test-configured-vitest-dir");
    write_package_json_with_vitest_dev_dependency(&root);
    fs::write(
        root.join("vitest.config.ts"),
        "export default { test: { dir: 'src' } };\n",
    )
    .expect("write config");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(root.join("src/example.test.ts"), "test('ok', () => {});\n").expect("write test");
    fs::write(root.join("decoy.test.ts"), "test('outside', () => {});\n").expect("write decoy");
    let args_log = root.join("vitest-args.log");
    install_local_vitest(
        &root,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"{}\"\nexit 0\n",
            args_log.display()
        ),
    );

    let plan = run_builtin_ok(root.to_path_buf(), "test", &["--plan"]);
    assert_output_contains_all(
        &plan,
        &[
            "vitest run --dir 'src'",
            "test.dir is `src`",
            "available-suites: vitest",
        ],
    );

    let out = run_builtin_ok(root, "test", &["--verbose-results"]);
    assert_output_contains_all(&out, &["Test Results", "command:vitest run --dir 'src'"]);
    let args = fs::read_to_string(&args_log).expect("read vitest args");
    assert!(
        args.lines().any(|line| line == "--dir") && args.lines().any(|line| line == "src"),
        "stub vitest should receive configured test dir, got {args:?}"
    );
}

#[test]
fn run_manifest_task_builtin_test_explicit_suite_override_stays_authoritative() {
    let root = temp_workspace("builtin-test-explicit-suite-over-vitest-dir");
    let configured_marker = root.join("configured-suite.log");
    let vitest_marker = root.join("vitest-suite.log");
    write_root_manifest(
        &root,
        &format!(
            r#"[test.suites]
unit = "sh -lc 'printf configured > \"{}\"'"
"#,
            configured_marker.display()
        ),
    );
    write_package_json_with_vitest_dev_dependency(&root);
    fs::write(
        root.join("vitest.config.ts"),
        "export default { test: { dir: 'src' } };\n",
    )
    .expect("write config");
    install_local_vitest_marker(&root, &vitest_marker);

    let plan = run_builtin_ok(root.to_path_buf(), "test", &["--plan"]);
    assert_output_contains_all(&plan, &["suite-source: configured", "test.suites.unit"]);
    assert_output_excludes_all(&plan, &["vitest run --dir", "auto-detected"]);

    let out = run_builtin_ok(root, "test", &["--verbose-results"]);
    assert_output_contains_all(&out, &["Test Results", "runner:unit"]);
    assert_path_exists(&configured_marker, "configured suite marker");
    assert_path_missing(&vitest_marker, "auto-detected vitest marker");
}
