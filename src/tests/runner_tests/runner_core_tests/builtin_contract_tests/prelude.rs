pub(super) use super::super::prelude::*;

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
