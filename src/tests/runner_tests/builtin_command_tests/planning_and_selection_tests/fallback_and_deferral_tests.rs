use super::super::prelude::*;

#[test]
fn run_manifest_task_explicit_test_task_overrides_builtin_auto_detection() {
    let root = temp_workspace("builtin-test-explicit-override");
    write_root_manifest(
        &root,
        "[tasks.test]\nrun = \"printf explicit > explicit-test.log\"\n",
    );
    write_package_json_with_test_script(&root);

    assert_builtin_ok_empty(root.clone(), "test", &[]);
    assert!(
        root.join("explicit-test.log").exists(),
        "explicit task should run before builtin test detection"
    );
}

#[test]
fn run_manifest_task_builtin_test_falls_through_to_deferral_when_no_detection_matches() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-defers");
    write_root_manifest(
        &root,
        "[defer]\nrun = \"test {request} = 'test' && test {args} = '--watch'\"\n",
    );

    assert_builtin_ok_empty(root, "test", &["--watch"]);
}
