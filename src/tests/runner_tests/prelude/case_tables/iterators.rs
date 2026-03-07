use super::super::errors::assert_invocation_error_contains;
use super::super::harness_builtin::{run_builtin_err, run_builtin_ok};
use super::super::harness_env::temp_workspace;
use super::super::harness_workspace::write_root_manifest;
use super::super::output::assert_output_contains_all;
use super::super::runtime::{Path, PathBuf};
use super::cases::{BuiltinInvocationCase, BuiltinInvocationOutcome, BuiltinInvocationSetupCase};

pub(super) fn workspace_with_root_manifest(workspace: &str, manifest: &str) -> PathBuf {
    let root = temp_workspace(workspace);
    write_root_manifest(&root, manifest);
    root
}

pub(super) fn for_each_workspace_case<T>(
    cases: &[T],
    workspace_of: impl Fn(&T) -> &str,
    mut setup: impl FnMut(&T, &Path),
    mut assert_case: impl FnMut(&T, PathBuf),
) {
    super::assertions::assert_case_table(cases.iter(), |case| {
        let root = temp_workspace(workspace_of(case));
        setup(case, &root);
        assert_case(case, root);
    });
}

pub(super) fn for_each_manifest_case<T>(
    cases: &[T],
    workspace_of: impl Fn(&T) -> &str,
    manifest_of: impl Fn(&T) -> &str,
    assert_case: impl FnMut(&T, PathBuf),
) {
    for_each_workspace_case(
        cases,
        workspace_of,
        |case, root| write_root_manifest(root, manifest_of(case)),
        assert_case,
    );
}

pub(super) fn invocation_outcome(expect_error: bool) -> BuiltinInvocationOutcome {
    if expect_error {
        BuiltinInvocationOutcome::Error
    } else {
        BuiltinInvocationOutcome::Success
    }
}

pub(super) fn assert_builtin_invocation(
    root: PathBuf,
    command: &str,
    args: &[&str],
    expected: &[&str],
    expected_outcome: BuiltinInvocationOutcome,
) {
    match expected_outcome {
        BuiltinInvocationOutcome::Success => {
            let out = run_builtin_ok(root, command, args);
            assert_output_contains_all(&out, expected);
        }
        BuiltinInvocationOutcome::Error => {
            let err = run_builtin_err(root, command, args);
            assert_invocation_error_contains(err, expected);
        }
    }
}

pub(super) fn assert_builtin_invocation_case_table_with_setup(
    command: &str,
    cases: &[BuiltinInvocationCase],
    setup: impl Fn(&Path),
    expected_outcome: BuiltinInvocationOutcome,
) {
    for_each_workspace_case(
        cases,
        |case| case.workspace,
        |_case, root| setup(root),
        |case, root| {
            assert_builtin_invocation(root, command, case.args, case.expected, expected_outcome)
        },
    );
}

pub(super) fn assert_builtin_invocation_case_table_with_case_setup(
    command: &str,
    cases: &[BuiltinInvocationSetupCase],
    expected_outcome: BuiltinInvocationOutcome,
) {
    for_each_workspace_case(
        cases,
        |case| case.workspace,
        |case, root| (case.setup)(root),
        |case, root| {
            assert_builtin_invocation(root, command, case.args, case.expected, expected_outcome)
        },
    );
}
