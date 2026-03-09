use super::prelude::{
    assert_output_contains_all, assert_output_excludes_all, assert_path_exists,
    assert_path_missing, fs, install_local_vitest_marker, lock_test, run_builtin_ok,
    setup_fanout_catalog_repo, temp_workspace, write_multi_suite_cargo_manifest,
    write_package_json_with_test_script,
};

#[test]
fn run_manifest_task_builtin_test_executes_local_vitest() {
    let root = temp_workspace("builtin-test-exec-vitest");
    let marker = root.join("vitest-called.log");
    write_package_json_with_test_script(&root);
    install_local_vitest_marker(&root, &marker);

    let out = run_builtin_ok(root.to_path_buf(), "test", &["--run"]);
    assert_output_contains_all(&out, &["Test Results", "targets:", "root"]);
    assert_output_excludes_all(&out, &["runner:vitest", "command:"]);
    assert_path_exists(&marker, "vitest stub marker");
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
    assert_output_contains_all(&out, &["Test Results", "root/vitest", "root/cargo-"]);
    assert_path_exists(&vitest_marker, "vitest suite marker");
}

#[test]
fn run_manifest_task_builtin_test_fans_out_across_catalog_roots() {
    let root = temp_workspace("builtin-test-fanout");
    let (catalog_a, catalog_b) = setup_fanout_catalog_repo(&root);
    let catalog_a_marker = catalog_a.join("vitest-called.log");
    let catalog_b_marker = catalog_b.join("vitest-called.log");
    install_local_vitest_marker(&catalog_a, &catalog_a_marker);
    install_local_vitest_marker(&catalog_b, &catalog_b_marker);

    let out = run_builtin_ok(root, "test", &[]);
    assert_output_contains_all(
        &out,
        &["Test Results", "targets:", "catalog_b", "catalog_a"],
    );
    assert_output_excludes_all(&out, &["runner:vitest", "command:"]);
    assert_path_exists(&catalog_a_marker, "catalog_a vitest marker");
    assert_path_exists(&catalog_b_marker, "catalog_b vitest marker");
}

#[test]
fn run_manifest_task_prefixed_builtin_test_targets_catalog_root_only() {
    let root = temp_workspace("builtin-test-prefixed-catalog");
    let (catalog_a, catalog_b) = setup_fanout_catalog_repo(&root);
    let catalog_a_marker = catalog_a.join("vitest-called.log");
    let catalog_b_marker = catalog_b.join("vitest-called.log");
    install_local_vitest_marker(&catalog_a, &catalog_a_marker);
    install_local_vitest_marker(&catalog_b, &catalog_b_marker);

    let out = run_builtin_ok(root, "catalog_a/test", &[]);
    assert_output_contains_all(&out, &["Test Results", "catalog_a"]);
    assert_output_excludes_all(&out, &["catalog_b"]);
    assert_path_exists(&catalog_a_marker, "catalog_a vitest marker");
    assert_path_missing(&catalog_b_marker, "catalog_b vitest marker");
}
