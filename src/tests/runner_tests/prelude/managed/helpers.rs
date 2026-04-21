use super::super::cases::assert_case_table;
use super::super::execution::run_manifest_task_with_cwd;
use super::super::output::{
    assert_output_contains_all, assert_output_contains_derived, assert_output_excludes_all,
    assert_path_exists, assert_path_missing,
};
use super::super::runtime::{fs, thread, Duration, Path, PathBuf, RunnerError, TaskInvocation};
use super::assertions::{
    assert_lock_conflict, assert_managed_non_zero_exit, assert_managed_profile_not_found,
    assert_unlock_invocation_error_contains,
};
use super::cases::{
    ManagedInvocation, ManagedNonZeroExitCase, ManagedOutputCase, ManagedOutputDerivedCase,
    ManagedProfileNotFoundCase, ManagedStreamBuiltinTestCase, ManagedUnlockInvocationErrorCase,
    ManagedUnlockSuccessCase,
};
use super::fixtures::{
    install_fake_container_runtime, write_managed_stream_builtin_test_manifest,
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

pub(in crate::runner::tests) fn run_dev(root: &Path, args: &[&str]) -> Result<String, RunnerError> {
    run_named_task(root, "dev", args)
}

pub(in crate::runner::tests) fn run_dev_with_repo(
    root: &Path,
    args: &[&str],
) -> Result<String, RunnerError> {
    run_named_task_with_repo(root, "dev", args)
}

pub(in crate::runner::tests) fn run_unlock_with_repo(
    root: &Path,
    scopes: &[&str],
) -> Result<String, RunnerError> {
    run_named_task_with_repo(root, "unlock", scopes)
}

pub(in crate::runner::tests) fn run_task_with_repo(
    root: &Path,
    name: &str,
    args: &[&str],
) -> Result<String, RunnerError> {
    run_named_task_with_repo(root, name, args)
}

pub(in crate::runner::tests) fn run_managed_invocation(
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

pub(in crate::runner::tests) fn assert_live_dev_lock_conflict(
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

fn workspace_root(workspace: &str) -> PathBuf {
    super::super::harness::temp_workspace(workspace)
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

pub(in crate::runner::tests) fn ensure_locks_dir(root: &Path) -> PathBuf {
    let locks_dir = root.join(".effigy/locks");
    fs::create_dir_all(&locks_dir).expect("mkdir locks");
    locks_dir
}

pub(in crate::runner::tests) fn write_lock_files(root: &Path, files: &[(&str, &str)]) {
    let locks_dir = ensure_locks_dir(root);
    for (name, body) in files {
        fs::write(locks_dir.join(name), body).expect("write lock file");
    }
}

pub(in crate::runner::tests) fn assert_lock_files_missing(root: &Path, files: &[&str]) {
    let locks_dir = root.join(".effigy/locks");
    for name in files {
        assert_path_missing(&locks_dir.join(name), "lock file");
    }
}

pub(in crate::runner::tests) fn assert_unlock_invocation_error_case_table(
    cases: &[ManagedUnlockInvocationErrorCase],
) {
    for_each_workspace_case(
        cases,
        |case| case.workspace,
        |root, case| {
            prepare_unlock_workspace(&root, None);
            let err = run_unlock_with_repo(&root, case.args).expect_err("unlock should fail");
            assert_unlock_invocation_error_contains(err, case.expected);
        },
    );
}

pub(in crate::runner::tests) fn assert_unlock_success_case_table(
    cases: &[ManagedUnlockSuccessCase],
) {
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

fn setup_workspace_with(workspace: &str, setup: fn(&Path)) -> PathBuf {
    let root = workspace_root(workspace);
    setup(&root);
    root
}

fn run_managed_case(
    workspace: &str,
    setup: fn(&Path),
    invocation: &ManagedInvocation,
    args: &[&str],
) -> (PathBuf, Result<String, RunnerError>) {
    let root = setup_workspace_with(workspace, setup);
    let _runtime = install_fake_container_runtime(&root);
    let result = run_managed_invocation(&root, invocation, args);
    (root, result)
}

fn assert_managed_case_table<T>(
    cases: &[T],
    workspace_of: impl Fn(&T) -> &str,
    setup_of: impl Fn(&T) -> fn(&Path),
    invocation_of: impl Fn(&T) -> &ManagedInvocation,
    args_of: impl Fn(&T) -> &[&str],
    mut assert_case: impl FnMut(&T, PathBuf, Result<String, RunnerError>),
) {
    assert_case_table(cases.iter(), |case| {
        let (root, result) = run_managed_case(
            workspace_of(case),
            setup_of(case),
            invocation_of(case),
            args_of(case),
        );
        assert_case(case, root, result);
    });
}

fn assert_managed_output_contract(out: &str, expected: &[&str], expected_absent: &[&str]) {
    assert_output_contains_all(out, expected);
    assert_output_excludes_all(out, expected_absent);
}

pub(in crate::runner::tests) fn assert_managed_output_case_table(cases: &[ManagedOutputCase]) {
    assert_managed_case_table(
        cases,
        |case| case.workspace,
        |case| case.setup,
        |case| &case.invocation,
        |case| case.args,
        |case, _root, out| {
            let out = out.expect("managed plan should render");
            assert_managed_output_contract(&out, case.expected, case.expected_absent);
        },
    );
}

pub(in crate::runner::tests) fn assert_managed_output_derived_case_table(
    cases: &[ManagedOutputDerivedCase],
) {
    assert_managed_case_table(
        cases,
        |case| case.workspace,
        |case| case.setup,
        |case| &case.invocation,
        |case| case.args,
        |case, root, out| {
            let out = out.expect("managed plan should render");
            assert_managed_output_contract(&out, case.expected, case.expected_absent);
            assert_output_contains_derived(&out, (case.expected_derived)(&root));
        },
    );
}

pub(in crate::runner::tests) fn assert_managed_profile_not_found_case_table(
    cases: &[ManagedProfileNotFoundCase],
) {
    assert_managed_case_table(
        cases,
        |case| case.workspace,
        |case| case.setup,
        |case| &case.invocation,
        |case| case.args,
        |case, _root, result| {
            let err = result.expect_err("expected missing managed profile error");
            assert_managed_profile_not_found(
                err,
                case.expected_task,
                case.expected_profile,
                case.expected_available,
            );
        },
    );
}

pub(in crate::runner::tests) fn assert_managed_non_zero_exit_case_table(
    cases: &[ManagedNonZeroExitCase],
) {
    assert_managed_case_table(
        cases,
        |case| case.workspace,
        |case| case.setup,
        |case| &case.invocation,
        |case| case.args,
        |case, _root, result| {
            let err = result.expect_err("expected managed non-zero exit error");
            assert_managed_non_zero_exit(
                err,
                case.expected_task,
                case.expected_profile,
                case.expected_processes,
            );
        },
    );
}

pub(in crate::runner::tests) fn assert_managed_stream_builtin_test_case_table(
    cases: &[ManagedStreamBuiltinTestCase],
) {
    for_each_workspace_case(
        cases,
        |case| case.workspace,
        |root, case| {
            let marker = root.join("builtin-test-called.log");
            write_managed_stream_builtin_test_manifest(&root, case.suite, case.task_ref, &marker);

            let out = run_dev(&root, &["default"])
                .expect("run managed stream with builtin profile entry");
            assert_output_contains_all(&out, &["Managed Task Runtime", "root: ok"]);
            assert_path_exists(&marker, "built-in test task ref marker");
        },
    );
}
