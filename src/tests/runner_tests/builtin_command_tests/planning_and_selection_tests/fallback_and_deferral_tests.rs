use crate::runner::tests::prelude::harness::*;
use crate::runner::tests::prelude::output::*;

#[test]
fn run_manifest_task_rejects_test_override_without_executing_plan() {
    let root = temp_workspace("builtin-test-removed-override");
    write_root_manifest(
        &root,
        "[tasks.test]\nrun = \"printf explicit > explicit-test.log\"\n",
    );
    write_package_json_with_test_script(&root);

    let error = run_builtin_err(root.to_path_buf(), "test", &["--plan"]);
    let rendered = error.to_string();
    assert!(rendered.contains("`tasks.test` was removed in v0.11"));
    assert!(rendered.contains("`[test.suites]`"));
    assert_path_missing(
        &root.join("explicit-test.log"),
        "removed test override marker",
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
