use super::prelude::*;

#[test]
fn run_manifest_task_builtin_test_executes_local_vitest() {
    let root = temp_workspace("builtin-test-exec-vitest");
    let marker = root.join("vitest-called.log");
    write_package_json_with_test_script(&root);
    install_local_vitest_marker(&root, &marker);

    let out = run_builtin_ok(root.to_path_buf(), "test", &["--run"]);
    assert_contains_all(&out, &["Test Results", "targets:", "root"]);
    assert!(!out.contains("runner:vitest"));
    assert!(!out.contains("command:"));
    assert!(marker.exists(), "vitest stub should be invoked");
}

#[test]
fn run_manifest_task_builtin_test_executes_js_and_rust_suites_in_same_repo() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-multi-context");
    write_package_json_with_test_script(&root);
    write_multi_suite_cargo_manifest(&root);
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("src/lib.rs"),
        "pub fn ok() -> bool { true }\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn smoke() {\n        assert!(super::ok());\n    }\n}\n",
    )
    .expect("write lib");

    let vitest_marker = root.join("vitest-called.log");
    install_local_vitest_marker(&root, &vitest_marker);

    let out = run_builtin_ok(root.to_path_buf(), "test", &[]);
    assert_contains_all(&out, &["Test Results", "root/vitest", "root/cargo-"]);
    assert!(vitest_marker.exists(), "vitest suite should run");
}

#[test]
fn run_manifest_task_builtin_test_fans_out_across_catalog_roots() {
    let root = temp_workspace("builtin-test-fanout");
    let (farmyard, dairy) = setup_fanout_catalog_repo(&root);
    let farmyard_marker = farmyard.join("vitest-called.log");
    let dairy_marker = dairy.join("vitest-called.log");
    install_local_vitest_marker(&farmyard, &farmyard_marker);
    install_local_vitest_marker(&dairy, &dairy_marker);

    let out = run_builtin_ok(root, "test", &[]);
    assert_contains_all(&out, &["Test Results", "targets:", "dairy", "farmyard"]);
    assert!(!out.contains("runner:vitest"));
    assert!(!out.contains("command:"));
    assert!(farmyard_marker.exists(), "farmyard vitest should run");
    assert!(dairy_marker.exists(), "dairy vitest should run");
}

#[test]
fn run_manifest_task_prefixed_builtin_test_targets_catalog_root_only() {
    let root = temp_workspace("builtin-test-prefixed-catalog");
    let (farmyard, dairy) = setup_fanout_catalog_repo(&root);
    let farmyard_marker = farmyard.join("vitest-called.log");
    let dairy_marker = dairy.join("vitest-called.log");
    install_local_vitest_marker(&farmyard, &farmyard_marker);
    install_local_vitest_marker(&dairy, &dairy_marker);

    let out = run_builtin_ok(root, "farmyard/test", &[]);
    assert_contains_all(&out, &["Test Results", "farmyard"]);
    assert!(!out.contains("dairy"));
    assert!(farmyard_marker.exists(), "farmyard vitest should run");
    assert!(!dairy_marker.exists(), "dairy vitest should not run");
}
