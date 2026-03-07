use super::{
    assert_lock_conflict, run_manifest_task_with_cwd, thread, Duration, ManagedInvocation, Path,
    RunnerError, TaskInvocation,
};

fn task_args(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_owned()).collect()
}

fn repo_scoped_args(root: &Path, args: &[&str]) -> Vec<String> {
    let mut full_args = vec!["--repo".to_owned(), root.display().to_string()];
    full_args.extend(task_args(args));
    full_args
}

fn run_named_task(root: &Path, name: &str, args: &[&str]) -> Result<String, RunnerError> {
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: name.to_owned(),
            args: task_args(args),
        },
        root.to_path_buf(),
    )
}

fn run_named_task_with_repo(root: &Path, name: &str, args: &[&str]) -> Result<String, RunnerError> {
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: name.to_owned(),
            args: repo_scoped_args(root, args),
        },
        root.to_path_buf(),
    )
}

pub(crate) fn run_dev(root: &Path, args: &[&str]) -> Result<String, RunnerError> {
    run_named_task(root, "dev", args)
}

pub(crate) fn run_dev_with_repo(root: &Path, args: &[&str]) -> Result<String, RunnerError> {
    run_named_task_with_repo(root, "dev", args)
}

pub(crate) fn run_unlock_with_repo(root: &Path, scopes: &[&str]) -> Result<String, RunnerError> {
    run_named_task_with_repo(root, "unlock", scopes)
}

pub(crate) fn run_task_with_repo(
    root: &Path,
    name: &str,
    args: &[&str],
) -> Result<String, RunnerError> {
    run_named_task_with_repo(root, name, args)
}

pub(crate) fn run_managed_invocation(
    root: &Path,
    invocation: &ManagedInvocation,
    args: &[&str],
) -> Result<String, RunnerError> {
    match invocation {
        ManagedInvocation::Dev => run_dev(root, args),
        ManagedInvocation::DevWithRepo => run_dev_with_repo(root, args),
        ManagedInvocation::TaskWithRepo(task) => run_task_with_repo(root, task, args),
    }
}

pub(crate) fn assert_live_dev_lock_conflict(
    root: &Path,
    warmup_ms: u64,
    expected_scope: &str,
    expected_remediation: &str,
) {
    let root_for_thread = root.to_path_buf();
    let join = thread::spawn(move || run_dev(&root_for_thread, &[]));

    std::thread::sleep(Duration::from_millis(warmup_ms));

    let err = run_dev(root, &[]).expect_err("second run should conflict on lock");
    assert_lock_conflict(err, expected_scope, expected_remediation);

    join.join()
        .expect("thread join")
        .expect("first run should complete");
}
