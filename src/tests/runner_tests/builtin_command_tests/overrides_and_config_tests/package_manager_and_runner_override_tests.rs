use super::prelude::{
    assert_file_text_equals, assert_output_contains_all, fs, lock_test, run_builtin_ok,
    temp_workspace, write_executable, write_js_package_manager_manifest,
    write_package_json_with_vitest_dev_dependency, write_root_manifest, EnvGuard,
};

#[test]
fn run_manifest_task_builtin_test_plan_respects_configured_package_manager() {
    let root = temp_workspace("builtin-test-plan-package-manager");
    write_js_package_manager_manifest(&root, "pnpm");
    write_package_json_with_vitest_dev_dependency(&root);

    let out = run_builtin_ok(root, "test", &["--plan"]);
    assert_output_contains_all(&out, &["pnpm exec vitest run", "package_manager.js=pnpm"]);
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
        (
            "EFFIGY_TEST_BUN_ARGS_FILE",
            Some(args_log.display().to_string()),
        ),
    ]);

    let out = run_builtin_ok(root, "test", &["vitest"]);
    assert_output_contains_all(&out, &["Test Results"]);
    assert_file_text_equals(&args_log, "x\nvitest\nrun\n");
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
    assert_output_contains_all(
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
    assert_output_contains_all(
        &out,
        &[
            "npx vitest run --reporter=dot",
            "package_manager.js=bun",
            "test.runners.vitest command override applied",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_test_plan_has_blank_line_between_sections() {
    let root = temp_workspace("builtin-test-plan-section-spacing");
    write_package_json_with_vitest_dev_dependency(&root);

    let out = run_builtin_ok(root, "test", &["--plan"]);
    assert_output_contains_all(&out, &["\n\nTarget Summary\n", "\n\nTarget: root\n"]);
}
