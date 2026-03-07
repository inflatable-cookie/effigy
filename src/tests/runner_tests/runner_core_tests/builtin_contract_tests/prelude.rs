pub(super) use super::super::prelude::{
    assert_builtin_error_case, assert_builtin_help_case_table, assert_case_table,
    assert_parser_task_invocation_error, builtin_help_case, builtin_scan_subcommand_help_cases,
    builtin_shared_help_precedence_cases, parse_completion_contract_request,
    parse_config_contract_request, parse_unlock_contract_request, parse_watch_contract_request,
    string_args, BuiltinErrorCase, CompletionParseContract, ConfigParseContract, TaskInvocation,
};

pub(super) struct BuiltinContractErrorCase {
    pub(super) workspace: &'static str,
    pub(super) command: &'static str,
    pub(super) args: &'static [&'static str],
    pub(super) expected: &'static [&'static str],
}

pub(super) fn builtin_contract_error_case(
    workspace: &'static str,
    command: &'static str,
    args: &'static [&'static str],
    expected: &'static [&'static str],
) -> BuiltinContractErrorCase {
    BuiltinContractErrorCase {
        workspace,
        command,
        args,
        expected,
    }
}

pub(super) fn assert_builtin_error_contract_case_table(cases: &[BuiltinContractErrorCase]) {
    assert_case_table(cases.iter(), |case| {
        assert_builtin_error_case(&BuiltinErrorCase {
            workspace: case.workspace,
            command: case.command,
            args: case.args,
            manifest: "",
            expected: case.expected,
        });
    });
}

pub(super) fn builtin_shared_unknown_argument_cases(
    cache_workspace: &'static str,
    completion_workspace: &'static str,
    watch_workspace: &'static str,
    scan_workspace: &'static str,
    unlock_workspace: &'static str,
) -> Vec<BuiltinContractErrorCase> {
    vec![
        builtin_contract_error_case(
            cache_workspace,
            "cache",
            &["inspect", "--wat"],
            &["unknown argument(s) for built-in `cache`: --wat"],
        ),
        builtin_contract_error_case(
            completion_workspace,
            "completion",
            &["candidates", "--wat"],
            &["unknown argument(s) for built-in `completion`: candidates --wat"],
        ),
        builtin_contract_error_case(
            watch_workspace,
            "watch",
            &["--wat"],
            &["unknown argument(s) for built-in `watch`: --wat"],
        ),
        builtin_contract_error_case(
            scan_workspace,
            "scan",
            &["god-files", "--wat"],
            &["unknown argument(s) for built-in `scan`: --wat"],
        ),
        builtin_contract_error_case(
            unlock_workspace,
            "unlock",
            &["--wat"],
            &["unknown argument(s) for built-in `unlock`: --wat"],
        ),
    ]
}
