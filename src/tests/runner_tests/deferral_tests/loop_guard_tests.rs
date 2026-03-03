use super::prelude::*;

#[test]
fn run_manifest_task_deferral_loop_guard_fails() {
    let _guard = lock_test();
    let root = temp_workspace("defer-loop");
    write_defer_manifest(&root, "printf deferred");

    let _env = EnvGuard::set_many(&[("EFFIGY_DEFER_DEPTH", Some("1".to_owned()))]);
    let err = run_task_in_workspace(&root, "unknown-task", &[]).expect_err("loop guard should fail");
    assert_defer_loop_detected(err, 1);
}
