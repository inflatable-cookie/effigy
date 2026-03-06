use super::prelude::{
    assert_watch_lock_conflict_case_table, assert_watch_output_case_table,
    write_build_task_manifest, write_root_manifest, Path, WatchLockConflictCase, WatchOutputCase,
};

fn setup_watch_once_target(root: &Path, marker: &Path) {
    write_root_manifest(
        root,
        &format!(
            "[tasks.build]\nrun = \"printf watched > '{}'\"\n",
            marker.display()
        ),
    );
}

fn setup_watch_lock_conflict_target(root: &Path) {
    write_build_task_manifest(root, "sleep 2");
}

#[test]
fn run_manifest_task_builtin_watch_once_executes_target_task() {
    let cases = [WatchOutputCase {
        workspace: "builtin-watch-once-exec",
        args: &["--owner", "effigy", "--once", "build"],
        marker_rel: "watch-once.log",
        expected: &["watch complete after 1 run(s)."],
        setup: setup_watch_once_target,
    }];

    assert_watch_output_case_table(&cases);
}

#[test]
fn run_manifest_task_builtin_watch_rejects_concurrent_watch_owner_for_same_target() {
    let cases = [WatchLockConflictCase {
        workspace: "builtin-watch-lock-conflict",
        args: &["--owner", "effigy", "--once", "build"],
        lock_rel: ".effigy/locks/task-watch-build.lock",
        lock_label: "watch lock for owner=effigy target=build",
        expected_scope: "task:watch:build",
        expected_remediation: "effigy unlock task:watch:build",
        setup: setup_watch_lock_conflict_target,
    }];

    assert_watch_lock_conflict_case_table(&cases);
}
