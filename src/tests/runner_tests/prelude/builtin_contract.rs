use super::cases::{assert_builtin_error_case, assert_case_table, BuiltinErrorCase};

pub(in crate::runner::tests) struct BuiltinContractErrorCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) command: &'static str,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) expected: &'static [&'static str],
}

pub(in crate::runner::tests) fn builtin_contract_error_case(
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

pub(in crate::runner::tests) fn assert_builtin_error_contract_case_table(
    cases: &[BuiltinContractErrorCase],
) {
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

pub(in crate::runner::tests) fn builtin_shared_unknown_argument_cases(
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
            &["unknown argument(s) for built-in `tasks cache`: --wat"],
        ),
        builtin_contract_error_case(
            completion_workspace,
            "completion",
            &["candidates", "--wat"],
            &["unknown argument(s) for built-in `config completion`: candidates --wat"],
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
            &["unknown argument(s) for built-in `tasks unlock`: --wat"],
        ),
    ]
}
