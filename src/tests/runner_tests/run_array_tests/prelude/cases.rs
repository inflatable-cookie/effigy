use super::{Path, RunnerError};

pub(in crate::runner::tests::run_array_tests) struct RunArrayInvocationErrorCase {
    pub(in crate::runner::tests::run_array_tests) workspace: &'static str,
    pub(in crate::runner::tests::run_array_tests) manifest: &'static str,
    pub(in crate::runner::tests::run_array_tests) expected: &'static [&'static str],
}

pub(in crate::runner::tests::run_array_tests) struct RunArrayValidateOutputCase {
    pub(in crate::runner::tests::run_array_tests) workspace: &'static str,
    pub(in crate::runner::tests::run_array_tests) manifest: &'static str,
    pub(in crate::runner::tests::run_array_tests) args: &'static [&'static str],
    pub(in crate::runner::tests::run_array_tests) expected: &'static [&'static str],
}

pub(in crate::runner::tests::run_array_tests) struct RunArrayTaskOutputCase {
    pub(in crate::runner::tests::run_array_tests) workspace: &'static str,
    pub(in crate::runner::tests::run_array_tests) task: &'static str,
    pub(in crate::runner::tests::run_array_tests) marker_rel: &'static str,
    pub(in crate::runner::tests::run_array_tests) expected: &'static str,
    pub(in crate::runner::tests::run_array_tests) setup: fn(&Path, &Path),
}

pub(in crate::runner::tests::run_array_tests) struct RunArrayTaskOutputDerivedCase {
    pub(in crate::runner::tests::run_array_tests) workspace: &'static str,
    pub(in crate::runner::tests::run_array_tests) task: &'static str,
    pub(in crate::runner::tests::run_array_tests) marker_rel: &'static str,
    pub(in crate::runner::tests::run_array_tests) expected: fn(&Path) -> String,
    pub(in crate::runner::tests::run_array_tests) setup: fn(&Path, &Path),
}

pub(in crate::runner::tests::run_array_tests) struct RunArrayTaskInvocationErrorCase {
    pub(in crate::runner::tests::run_array_tests) workspace: &'static str,
    pub(in crate::runner::tests::run_array_tests) task: &'static str,
    pub(in crate::runner::tests::run_array_tests) expected: &'static [&'static str],
    pub(in crate::runner::tests::run_array_tests) setup: fn(&Path),
}

pub(in crate::runner::tests::run_array_tests) struct RunArrayTaskRefParseErrorCase {
    pub(in crate::runner::tests::run_array_tests) workspace: &'static str,
    pub(in crate::runner::tests::run_array_tests) manifest: &'static str,
    pub(in crate::runner::tests::run_array_tests) expected_tail: &'static str,
}

pub(in crate::runner::tests::run_array_tests) struct RunArrayInvocationMessageCase {
    pub(in crate::runner::tests::run_array_tests) workspace: &'static str,
    pub(in crate::runner::tests::run_array_tests) manifest: &'static str,
    pub(in crate::runner::tests::run_array_tests) args: &'static [&'static str],
    pub(in crate::runner::tests::run_array_tests) expected_all: &'static [&'static str],
    pub(in crate::runner::tests::run_array_tests) expected_any: &'static [&'static str],
    pub(in crate::runner::tests::run_array_tests) expected_exact: Option<&'static str>,
}

pub(in crate::runner::tests::run_array_tests) struct BuiltinTestTaskRefCase {
    pub(in crate::runner::tests::run_array_tests) workspace: &'static str,
    pub(in crate::runner::tests::run_array_tests) suite_name: &'static str,
    pub(in crate::runner::tests::run_array_tests) task_ref: &'static str,
}

pub(in crate::runner::tests::run_array_tests) struct RunArrayValidateMarkerCase {
    pub(in crate::runner::tests::run_array_tests) workspace: &'static str,
    pub(in crate::runner::tests::run_array_tests) args: &'static [&'static str],
    pub(in crate::runner::tests::run_array_tests) marker_rel: &'static str,
    pub(in crate::runner::tests::run_array_tests) expected: &'static [&'static str],
    pub(in crate::runner::tests::run_array_tests) setup: fn(&Path, &Path),
}

pub(in crate::runner::tests::run_array_tests) fn run_validate_err_invocation_message(
    root: &Path,
    args: &[&str],
) -> String {
    match super::run_validate_err(root, args) {
        RunnerError::TaskInvocation(message) => message,
        other => panic!("unexpected error: {other}"),
    }
}
