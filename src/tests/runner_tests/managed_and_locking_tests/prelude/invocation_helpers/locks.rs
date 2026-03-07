use super::{
    assert_case_table, assert_invocation_error_contains, assert_output_contains_all,
    assert_path_missing, fs, run_unlock_with_repo, temp_workspace,
    ManagedUnlockInvocationErrorCase, ManagedUnlockSuccessCase, Path, PathBuf,
};

fn workspace_root(workspace: &str) -> PathBuf {
    temp_workspace(workspace)
}

fn for_each_workspace_case<T>(
    cases: &[T],
    workspace_of: impl Fn(&T) -> &str,
    mut visit: impl FnMut(PathBuf, &T),
) {
    assert_case_table(cases.iter(), |case| {
        let root = workspace_root(workspace_of(case));
        visit(root, case);
    });
}

fn prepare_unlock_workspace(root: &Path, lock_files: Option<&[(&str, &str)]>) {
    match lock_files {
        Some(lock_files) => write_lock_files(root, lock_files),
        None => {
            ensure_locks_dir(root);
        }
    }
}

pub(crate) fn ensure_locks_dir(root: &Path) -> PathBuf {
    let locks_dir = root.join(".effigy/locks");
    fs::create_dir_all(&locks_dir).expect("mkdir locks");
    locks_dir
}

pub(crate) fn write_lock_files(root: &Path, files: &[(&str, &str)]) {
    let locks_dir = ensure_locks_dir(root);
    for (name, body) in files {
        fs::write(locks_dir.join(name), body).expect("write lock file");
    }
}

pub(crate) fn assert_lock_files_missing(root: &Path, files: &[&str]) {
    let locks_dir = root.join(".effigy/locks");
    for name in files {
        assert_path_missing(&locks_dir.join(name), "lock file");
    }
}

pub(crate) fn assert_unlock_invocation_error_case_table(
    cases: &[ManagedUnlockInvocationErrorCase],
) {
    for_each_workspace_case(
        cases,
        |case| case.workspace,
        |root, case| {
            prepare_unlock_workspace(&root, None);
            let err = run_unlock_with_repo(&root, case.args).expect_err("unlock should fail");
            assert_invocation_error_contains(err, case.expected);
        },
    );
}

pub(crate) fn assert_unlock_success_case_table(cases: &[ManagedUnlockSuccessCase]) {
    for_each_workspace_case(
        cases,
        |case| case.workspace,
        |root, case| {
            prepare_unlock_workspace(&root, Some(case.lock_files));
            let out = run_unlock_with_repo(&root, case.args).expect("unlock should run");
            assert_output_contains_all(&out, case.expected);
            assert_lock_files_missing(&root, case.removed_lock_files);
        },
    );
}
