use super::errors::{
    assert_invocation_error_contains, assert_runner_manifest_parse_error_contains_any,
};
use super::harness_builtin::{run_builtin_err, run_builtin_ok};
use super::harness_tasks::run_tasks_with_repo;
use super::harness_env::temp_workspace;
use super::harness_workspace::write_root_manifest;
use super::json::{
    assert_json_bool_field_eq, assert_json_string_field_eq, parse_json_output_with_schema_version,
};
use super::output::assert_output_contains_all;
use super::runtime::{Path, PathBuf, RunnerError};

#[derive(Clone, Copy)]
enum BuiltinInvocationOutcome {
    Success,
    Error,
}

pub(in crate::runner::tests) struct BuiltinErrorCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) command: &'static str,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) manifest: &'static str,
    pub(in crate::runner::tests) expected: &'static [&'static str],
}

pub(in crate::runner::tests) struct BuiltinHelpCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) command: &'static str,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) expected: &'static [&'static str],
}

pub(in crate::runner::tests) struct BuiltinArgumentContractCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) expect_error: bool,
    pub(in crate::runner::tests) expected: &'static [&'static str],
}

pub(in crate::runner::tests) struct BuiltinArgumentContractCommandCase<'a> {
    pub(in crate::runner::tests) command: &'a str,
    pub(in crate::runner::tests) cases: &'a [BuiltinArgumentContractCase],
}

pub(in crate::runner::tests) struct BuiltinInvocationCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) expected: &'static [&'static str],
}

pub(in crate::runner::tests) struct BuiltinInvocationSetupCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) expected: &'static [&'static str],
    pub(in crate::runner::tests) setup: fn(&Path),
}

pub(in crate::runner::tests) struct BuiltinHelpJsonCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) command: &'static str,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) expected_topic: &'static str,
    pub(in crate::runner::tests) expected_usage_fragment: &'static str,
}

pub(in crate::runner::tests) struct ManifestParseRejectionCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) manifest: &'static str,
    pub(in crate::runner::tests) expected: &'static [&'static str],
}

fn workspace_with_root_manifest(workspace: &str, manifest: &str) -> PathBuf {
    let root = temp_workspace(workspace);
    write_root_manifest(&root, manifest);
    root
}

fn for_each_workspace_case<T>(
    cases: &[T],
    workspace_of: impl Fn(&T) -> &str,
    mut setup: impl FnMut(&T, &Path),
    mut assert_case: impl FnMut(&T, PathBuf),
) {
    assert_case_table(cases.iter(), |case| {
        let root = temp_workspace(workspace_of(case));
        setup(case, &root);
        assert_case(case, root);
    });
}

fn for_each_manifest_case<T>(
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

fn invocation_outcome(expect_error: bool) -> BuiltinInvocationOutcome {
    if expect_error {
        BuiltinInvocationOutcome::Error
    } else {
        BuiltinInvocationOutcome::Success
    }
}

fn assert_builtin_invocation(
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

fn assert_builtin_invocation_case_table_with_setup(
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

fn assert_builtin_invocation_case_table_with_case_setup(
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
    setup: impl Fn(&Path),
) {
    assert_builtin_invocation_case_table_with_setup(
        command,
        cases,
        setup,
        BuiltinInvocationOutcome::Success,
    );
}

pub(in crate::runner::tests) fn assert_builtin_ok_case_table_with_case_setup(
    command: &str,
    cases: &[BuiltinInvocationSetupCase],
) {
    assert_builtin_invocation_case_table_with_case_setup(
        command,
        cases,
        BuiltinInvocationOutcome::Success,
    );
}

pub(in crate::runner::tests) fn assert_builtin_error_case_table_with_setup(
    command: &str,
    cases: &[BuiltinInvocationCase],
    setup: impl Fn(&Path),
) {
    assert_builtin_invocation_case_table_with_setup(
        command,
        cases,
        setup,
        BuiltinInvocationOutcome::Error,
    );
}

pub(in crate::runner::tests) fn assert_builtin_error_case_table_with_case_setup(
    command: &str,
    cases: &[BuiltinInvocationSetupCase],
) {
    assert_builtin_invocation_case_table_with_case_setup(
        command,
        cases,
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

pub(in crate::runner::tests) fn assert_parser_task_invocation_error<T>(
    result: Result<T, RunnerError>,
    expected: &str,
) {
    match result {
        Ok(_) => panic!("expected parser task invocation error containing `{expected}`"),
        Err(err) => assert_invocation_error_contains(err, &[expected]),
    }
}
