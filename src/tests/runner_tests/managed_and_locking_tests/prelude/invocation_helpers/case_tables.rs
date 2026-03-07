use super::{
    assert_case_table, assert_managed_non_zero_exit, assert_managed_profile_not_found,
    assert_output_contains_all, assert_output_contains_derived, assert_output_excludes_all,
    assert_path_exists, run_dev, run_managed_invocation, temp_workspace,
    write_managed_stream_builtin_test_manifest, ManagedInvocation, ManagedNonZeroExitCase,
    ManagedOutputCase, ManagedOutputDerivedCase, ManagedProfileNotFoundCase,
    ManagedStreamBuiltinTestCase, Path, PathBuf, RunnerError,
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

pub(crate) fn assert_managed_output_case_table(cases: &[ManagedOutputCase]) {
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

pub(crate) fn assert_managed_output_derived_case_table(cases: &[ManagedOutputDerivedCase]) {
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

pub(crate) fn assert_managed_profile_not_found_case_table(cases: &[ManagedProfileNotFoundCase]) {
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

pub(crate) fn assert_managed_non_zero_exit_case_table(cases: &[ManagedNonZeroExitCase]) {
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

pub(crate) fn assert_managed_stream_builtin_test_case_table(
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
