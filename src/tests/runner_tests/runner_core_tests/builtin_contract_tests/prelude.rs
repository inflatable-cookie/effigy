pub(super) use super::super::prelude::{
    assert_builtin_error_case, assert_builtin_help_case_table, assert_case_table,
    assert_parser_task_invocation_error, parse_completion_contract_request,
    parse_config_contract_request, parse_unlock_contract_request, parse_watch_contract_request,
    string_args, BuiltinErrorCase, BuiltinHelpCase, CompletionParseContract,
    ConfigParseContract, TaskInvocation,
};

pub(super) struct BuiltinContractErrorCase {
    pub(super) workspace: &'static str,
    pub(super) command: &'static str,
    pub(super) args: &'static [&'static str],
    pub(super) expected: &'static [&'static str],
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
