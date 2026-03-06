pub(super) use super::super::prelude::{
    assert_builtin_error_case_table, assert_case_table, assert_output_contains_all,
    assert_path_exists, assert_task_lock_conflict, lock_test, run_builtin_ok, run_task,
    temp_workspace, wait_for_path_exists, write_empty_manifest, write_root_manifest, thread,
    BuiltinErrorCase, Duration, Path, PathBuf, RunnerError,
};

pub(super) struct WatchOutputCase {
    pub(super) workspace: &'static str,
    pub(super) args: &'static [&'static str],
    pub(super) marker_rel: &'static str,
    pub(super) expected: &'static [&'static str],
    pub(super) setup: fn(&Path, &Path),
}

pub(super) struct WatchLockConflictCase {
    pub(super) workspace: &'static str,
    pub(super) args: &'static [&'static str],
    pub(super) lock_rel: &'static str,
    pub(super) lock_label: &'static str,
    pub(super) expected_scope: &'static str,
    pub(super) expected_remediation: &'static str,
    pub(super) setup: fn(&Path),
}

pub(super) fn write_build_task_manifest(root: &Path, run: &str) {
    write_root_manifest(root, &format!("[tasks.build]\nrun = \"{run}\"\n"));
}

pub(super) fn run_watch(root: &Path, args: &[&str]) -> Result<String, RunnerError> {
    run_task(root, "watch", args)
}

fn watch_workspace_with_marker(case: &WatchOutputCase) -> (PathBuf, PathBuf) {
    let root = temp_workspace(case.workspace);
    let marker = root.join(case.marker_rel);
    (case.setup)(&root, &marker);
    (root, marker)
}

fn watch_workspace(case: &WatchLockConflictCase) -> PathBuf {
    let root = temp_workspace(case.workspace);
    (case.setup)(&root);
    root
}

fn for_each_watch_output_case(
    cases: &[WatchOutputCase],
    mut assert_case: impl FnMut(&WatchOutputCase, PathBuf, PathBuf),
) {
    assert_case_table(cases.iter(), |case| {
        let (root, marker) = watch_workspace_with_marker(case);
        assert_case(case, root, marker);
    });
}

fn for_each_watch_lock_conflict_case(
    cases: &[WatchLockConflictCase],
    mut assert_case: impl FnMut(&WatchLockConflictCase, PathBuf),
) {
    assert_case_table(cases.iter(), |case| {
        let root = watch_workspace(case);
        assert_case(case, root);
    });
}

fn spawn_watch_owner(
    root: &Path,
    args: &'static [&'static str],
) -> thread::JoinHandle<Result<String, RunnerError>> {
    let root_for_thread = root.to_path_buf();
    thread::spawn(move || run_watch(&root_for_thread, args))
}

pub(super) fn assert_watch_output_case_table(cases: &[WatchOutputCase]) {
    for_each_watch_output_case(cases, |case, root, marker| {
        let out = run_watch(&root, case.args).expect("watch should run");
        assert_output_contains_all(&out, case.expected);
        assert_path_exists(&marker, "watch marker");
    });
}

pub(super) fn assert_watch_lock_conflict_case_table(cases: &[WatchLockConflictCase]) {
    for_each_watch_lock_conflict_case(cases, |case, root| {
        let _guard = lock_test();
        let join = spawn_watch_owner(&root, case.args);

        let watch_lock = root.join(case.lock_rel);
        wait_for_path_exists(&watch_lock, Duration::from_secs(5), case.lock_label);

        let err = run_watch(&root, case.args)
            .expect_err("second watch owner should conflict on watch scope lock");
        assert_task_lock_conflict(err, case.expected_scope, case.expected_remediation);

        let first = join.join().expect("thread join");
        first.expect("first watch should complete");
    });
}
