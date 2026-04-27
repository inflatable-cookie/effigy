use crate::runner::tests::prelude::{
    assert_task_not_found_any, lock_test, run_task_in_workspace,
    workspace_with_optional_defer_manifest, EnvGuard,
};

/// When we're already inside a deferred subprocess (`EFFIGY_DEFER_DEPTH`
/// set by the outer runner), the implicit fallback path must not attempt
/// another deferral. The cleanest signal in that case is the original
/// "task not found" error from the inner runner — propagating it directly
/// avoids a second round of container/composer setup and surfaces the
/// real cause (typically a typo'd task name) rather than masking it
/// behind a `DeferLoopDetected` that points the user at the wrong fix.
///
/// Defense-in-depth `DeferLoopDetected` still fires for the explicit
/// `effigy defer ...` path, which bypasses this policy and goes straight
/// to `run_deferred_request`.
#[test]
fn implicit_deferral_inside_deferred_subprocess_propagates_task_not_found() {
    let _guard = lock_test();
    let root = workspace_with_optional_defer_manifest("defer-loop", Some("printf deferred"), None);

    let _env = EnvGuard::set_many(&[("EFFIGY_DEFER_DEPTH", Some("1".to_owned()))]);
    let err = run_task_in_workspace(&root, "unknown-task", &[])
        .expect_err("nested deferral should not be attempted");
    assert_task_not_found_any(err);
}
