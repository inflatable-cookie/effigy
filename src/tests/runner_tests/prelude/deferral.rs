use super::cases::assert_case_table;
use super::fixture_support::{run_task_in_workspace, write_defer_manifest};
use super::harness::temp_workspace;
use super::output::assert_output_equals;
use super::runtime::{Path, PathBuf, RunnerError};

pub(in crate::runner::tests) struct DeferredTaskCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) defer_run: &'static str,
    pub(in crate::runner::tests) defer_run_in: Option<&'static str>,
    pub(in crate::runner::tests) request: &'static str,
    pub(in crate::runner::tests) args: &'static [&'static str],
}

pub(in crate::runner::tests) struct DeferredTaskFailureCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) defer_run: &'static str,
    pub(in crate::runner::tests) defer_run_in: &'static str,
    pub(in crate::runner::tests) request: &'static str,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) expected_message: &'static str,
}

pub(in crate::runner::tests) fn assert_task_not_found_any(err: RunnerError) {
    match err {
        RunnerError::TaskNotFoundAny { .. } => {}
        other => panic!("unexpected error: {other}"),
    }
}

pub(in crate::runner::tests) fn assert_defer_loop_detected(err: RunnerError, expected_depth: u8) {
    match err {
        RunnerError::DeferLoopDetected { depth } => assert_eq!(depth, expected_depth),
        other => panic!("unexpected error: {other}"),
    }
}

pub(in crate::runner::tests) fn workspace_with_optional_defer_manifest(
    workspace: &str,
    defer_run: Option<&str>,
    run_in: Option<&str>,
) -> PathBuf {
    let root = temp_workspace(workspace);
    if let Some(defer_run) = defer_run {
        write_defer_manifest(&root, defer_run, run_in);
    }
    root
}

fn for_each_workspace_case<T>(
    cases: &[T],
    workspace_of: impl Fn(&T) -> &str,
    defer_run_of: impl Fn(&T) -> Option<&str>,
    defer_run_in_of: impl Fn(&T) -> Option<&str>,
    mut assert_case: impl FnMut(&T, PathBuf),
) {
    assert_case_table(cases.iter(), |case| {
        let root = workspace_with_optional_defer_manifest(
            workspace_of(case),
            defer_run_of(case),
            defer_run_in_of(case),
        );
        assert_case(case, root);
    });
}

fn run_case_task(root: &Path, request: &str, args: &[&str]) -> Result<String, RunnerError> {
    run_task_in_workspace(root, request, args)
}

pub(in crate::runner::tests) fn run_task_expect_empty_output(
    root: &Path,
    request: &str,
    args: &[&str],
    context: &str,
) {
    let out = run_case_task(root, request, args).expect(context);
    assert_output_equals(&out, "");
}

pub(in crate::runner::tests) fn assert_deferred_task_case_table(cases: &[DeferredTaskCase]) {
    for_each_workspace_case(
        cases,
        |case| case.workspace,
        |case| Some(case.defer_run),
        |case| case.defer_run_in,
        |case, root| {
            run_task_expect_empty_output(
                &root,
                case.request,
                case.args,
                "deferred run should succeed",
            );
        },
    );
}

pub(in crate::runner::tests) fn assert_deferred_task_failure_case_table(
    cases: &[DeferredTaskFailureCase],
) {
    for_each_workspace_case(
        cases,
        |case| case.workspace,
        |case| Some(case.defer_run),
        |case| Some(case.defer_run_in),
        |case, root| {
            let err = run_case_task(&root, case.request, case.args)
                .expect_err("deferred run should fail");
            match err {
                RunnerError::TaskInvocation(message) => {
                    assert!(
                        message.contains(case.expected_message),
                        "expected `{}` in `{message}`",
                        case.expected_message
                    );
                }
                other => panic!("unexpected error: {other}"),
            }
        },
    );
}
