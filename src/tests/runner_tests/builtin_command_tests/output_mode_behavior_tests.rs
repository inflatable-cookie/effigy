use super::prelude::{
    assert_json_bool_field_eq, assert_json_string_field_eq, assert_output_contains_all,
    assert_output_excludes_all, install_local_vitest, install_local_vitest_marker,
    parse_json_output_with_schema, run_builtin_ok, temp_workspace, write_executable,
    write_package_json_with_test_script, write_root_manifest, EnvGuard,
};

#[test]
fn run_manifest_task_builtin_test_json_suppresses_child_process_output() {
    let root = temp_workspace("builtin-test-json-suppresses-child-output");
    write_package_json_with_test_script(&root);
    install_local_vitest(
        &root,
        "#!/bin/sh\nprintf noisy-stdout\nprintf noisy-stderr >&2\nexit 0\n",
    );

    let out = run_builtin_ok(root, "test", &["--json", "--run"]);

    assert_output_excludes_all(&out, &["noisy-stdout", "noisy-stderr"]);
    let _parsed = parse_json_output_with_schema(&out, "effigy.test.results.v1");
}

#[test]
fn run_manifest_task_builtin_test_text_and_json_outputs_share_target_identity() {
    let root = temp_workspace("builtin-test-json-text-target-parity");
    write_package_json_with_test_script(&root);
    let marker = root.join("vitest-called.log");
    install_local_vitest_marker(&root, &marker);

    let text = run_builtin_ok(root.to_path_buf(), "test", &["--run"]);
    assert_output_contains_all(&text, &["Test Results", "root", "ok"]);

    let json = run_builtin_ok(root, "test", &["--json", "--run"]);
    let parsed = parse_json_output_with_schema(&json, "effigy.test.results.v1");
    assert_json_string_field_eq(&parsed["targets"][0], "target", "root");
    assert_json_string_field_eq(&parsed["targets"][0], "cargo_env_match", "prefix-aware");
    assert_json_bool_field_eq(&parsed["targets"][0], "success", true);
}

#[test]
fn run_manifest_task_builtin_test_verbose_results_include_runner_root_and_command() {
    let root = temp_workspace("builtin-test-verbose-results");
    write_package_json_with_test_script(&root);
    install_local_vitest(&root, "#!/bin/sh\nexit 0\n");

    let out = run_builtin_ok(root, "test", &["--verbose-results", "--run"]);
    assert_output_contains_all(
        &out,
        &[
            "Test Results",
            "runner:vitest",
            "root:",
            "cargo-env-match:prefix-aware",
            "command:vitest run '--run'",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_test_verbose_results_and_json_report_shell_aware_cargo_env_match() {
    let root = temp_workspace("builtin-test-results-cargo-env-match-shell-aware");
    write_root_manifest(
        &root,
        r#"[test]
cargo_env_match = "shell-aware"

[test.suites]
integration = "sh -lc 'exit 0'"
"#,
    );

    let text = run_builtin_ok(root.to_path_buf(), "test", &["--verbose-results"]);
    assert_output_contains_all(&text, &["Test Results", "cargo-env-match:shell-aware"]);

    let json = run_builtin_ok(root, "test", &["--json"]);
    let parsed = parse_json_output_with_schema(&json, "effigy.test.results.v1");
    assert_json_string_field_eq(&parsed["targets"][0], "cargo_env_match", "shell-aware");
}

#[test]
fn run_manifest_task_builtin_test_tui_flag_falls_back_to_text_when_non_interactive() {
    let root = temp_workspace("builtin-test-tui-fallback");
    write_package_json_with_test_script(&root);
    install_local_vitest(&root, "#!/bin/sh\nexit 0\n");

    let out = run_builtin_ok(root, "test", &["--tui"]);
    assert_output_contains_all(&out, &["Test Results", "root"]);
}

#[test]
fn run_manifest_task_builtin_test_verbose_results_and_json_report_suite_lifecycle_metadata() {
    let root = temp_workspace("builtin-test-results-suite-lifecycle-metadata");
    write_root_manifest(
        &root,
        r#"[test.suites.managed]
run = "suite-run"
env = "TEST_DATABASE_URL"
env_file = ".env.test"
setup = [{ run = "printf prep >/dev/null" }]
teardown = [{ run = "printf clean >/dev/null" }]
teardown_policy = "always"
"#,
    );
    std::fs::write(root.join(".env.test"), "TEST_DATABASE_URL=test-db\n").expect("write env");
    let bin_dir = root.join("bin");
    std::fs::create_dir_all(&bin_dir).expect("mkdir bin");
    write_executable(&bin_dir.join("suite-run"), "#!/bin/sh\nexit 0\n");
    let prior_path = std::env::var("PATH").ok().unwrap_or_default();
    let _env = EnvGuard::set_many(&[("PATH", Some(format!("{}:{prior_path}", bin_dir.display())))]);

    let text = run_builtin_ok(root.to_path_buf(), "test", &["--verbose-results"]);
    assert_output_contains_all(
        &text,
        &[
            "suite-env:profile:TEST_DATABASE_URL",
            "suite-env-files:.env.test",
            "setup-steps:1",
            "teardown-steps:1",
            "teardown-policy:always",
        ],
    );

    let json = run_builtin_ok(root, "test", &["--json"]);
    let parsed = parse_json_output_with_schema(&json, "effigy.test.results.v1");
    assert_json_string_field_eq(
        &parsed["targets"][0],
        "suite_env",
        "profile:TEST_DATABASE_URL",
    );
    assert_json_string_field_eq(&parsed["targets"][0], "teardown_policy", "always");
    assert_eq!(
        parsed["targets"][0]["suite_env_files"]
            .as_array()
            .expect("suite_env_files array")
            .iter()
            .filter_map(|value| value.as_str())
            .collect::<Vec<&str>>(),
        vec![".env.test"]
    );
    assert_eq!(parsed["targets"][0]["setup_steps"].as_u64(), Some(1));
    assert_eq!(parsed["targets"][0]["teardown_steps"].as_u64(), Some(1));
}
