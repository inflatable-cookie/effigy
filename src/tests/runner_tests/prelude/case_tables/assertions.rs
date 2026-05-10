use super::super::errors::{
    assert_invocation_error_contains, assert_runner_manifest_parse_error_contains_any,
};
use super::super::harness_builtin::run_builtin_ok;
use super::super::harness_tasks::run_tasks_with_repo;
use super::super::json::{
    assert_json_bool_field_eq, assert_json_string_field_eq, parse_json_output_with_schema_version,
};
use super::super::runtime::RunnerError;
use super::cases::{
    BuiltinArgumentContractCase, BuiltinArgumentContractCommandCase, BuiltinErrorCase,
    BuiltinHelpCase, BuiltinHelpJsonCase, BuiltinInvocationCase, BuiltinInvocationOutcome,
    ManifestParseRejectionCase,
};
use super::iterators::{
    assert_builtin_invocation, assert_builtin_invocation_case_table_with_setup,
    for_each_manifest_case, invocation_outcome, workspace_with_root_manifest,
};

pub(in crate::runner::tests) fn assert_builtin_help_case(case: &BuiltinHelpCase) {
    let root = workspace_with_root_manifest(case.workspace, "");
    assert_builtin_invocation(
        root,
        case.command,
        case.args,
        case.expected,
        BuiltinInvocationOutcome::Success,
    );
}

pub(in crate::runner::tests) fn assert_builtin_help_case_table(cases: &[BuiltinHelpCase]) {
    assert_case_table(cases.iter(), |case| {
        assert_builtin_help_case(case);
    });
}

pub(in crate::runner::tests) fn assert_case_table<T>(
    cases: impl IntoIterator<Item = T>,
    mut assert_case: impl FnMut(T),
) {
    for case in cases {
        assert_case(case);
    }
}

pub(in crate::runner::tests) fn assert_builtin_error_case(case: &BuiltinErrorCase) {
    let root = workspace_with_root_manifest(case.workspace, case.manifest);
    assert_builtin_invocation(
        root,
        case.command,
        case.args,
        case.expected,
        BuiltinInvocationOutcome::Error,
    );
}

pub(in crate::runner::tests) fn assert_builtin_error_case_table(cases: &[BuiltinErrorCase]) {
    assert_case_table(cases.iter(), |case| {
        assert_builtin_error_case(case);
    });
}

pub(in crate::runner::tests) fn assert_builtin_argument_contract_case_table(
    command: &str,
    cases: &[BuiltinArgumentContractCase],
) {
    for_each_manifest_case(
        cases,
        |case| case.workspace,
        |_case| "",
        |case, root| {
            let outcome = invocation_outcome(case.expect_error);
            assert_builtin_invocation(root, command, case.args, case.expected, outcome);
        },
    );
}

pub(in crate::runner::tests) fn assert_builtin_argument_contract_command_case_table(
    command_cases: &[BuiltinArgumentContractCommandCase<'_>],
) {
    assert_case_table(command_cases.iter(), |case| {
        assert_builtin_argument_contract_case_table(case.command, case.cases);
    });
}

pub(in crate::runner::tests) fn assert_builtin_ok_case_table_with_setup(
    command: &str,
    cases: &[BuiltinInvocationCase],
    setup: impl Fn(&std::path::Path),
) {
    assert_builtin_invocation_case_table_with_setup(
        command,
        cases,
        setup,
        BuiltinInvocationOutcome::Success,
    );
}

pub(in crate::runner::tests) fn assert_builtin_error_case_table_with_setup(
    command: &str,
    cases: &[BuiltinInvocationCase],
    setup: impl Fn(&std::path::Path),
) {
    assert_builtin_invocation_case_table_with_setup(
        command,
        cases,
        setup,
        BuiltinInvocationOutcome::Error,
    );
}

pub(in crate::runner::tests) fn assert_builtin_help_json_contract_case_table(
    cases: &[BuiltinHelpJsonCase],
) {
    for_each_manifest_case(
        cases,
        |case| case.workspace,
        |_case| "",
        |case, root| {
            let out = run_builtin_ok(root, case.command, case.args);
            let parsed = parse_json_output_with_schema_version(&out, "effigy.help.v1", 1);
            assert_json_bool_field_eq(&parsed, "ok", true);
            assert_json_string_field_eq(&parsed, "topic", case.expected_topic);
            let text = parsed["text"].as_str().expect("help text string");
            assert!(
                text.contains(case.expected_usage_fragment),
                "expected help text for {} to include {:?}, got:\n{}",
                case.command,
                case.expected_usage_fragment,
                text
            );
        },
    );
}

pub(in crate::runner::tests) fn assert_tasks_manifest_parse_rejection_case_table(
    cases: &[ManifestParseRejectionCase],
) {
    for_each_manifest_case(
        cases,
        |case| case.workspace,
        |case| case.manifest,
        |case, root| {
            let err = run_tasks_with_repo(root).expect_err("expected manifest parse failure");
            assert_runner_manifest_parse_error_contains_any(err, case.expected);
        },
    );
}

pub(in crate::runner::tests) fn string_args(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_owned()).collect()
}

pub(in crate::runner::tests) fn assert_parser_task_invocation_error<T, E>(
    result: Result<T, E>,
    expected: &str,
) where
    E: Into<RunnerError>,
{
    match result {
        Ok(_) => panic!("expected parser task invocation error containing `{expected}`"),
        Err(err) => assert_invocation_error_contains(err.into(), &[expected]),
    }
}
