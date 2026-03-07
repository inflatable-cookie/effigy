pub(super) use super::super::prelude::{
    assert_builtin_error_case_table_with_setup, assert_builtin_help_case_table,
    assert_builtin_help_json_contract_case_table, assert_builtin_ok_case_table_with_setup,
    builtin_help_case, builtin_help_json_case, builtin_invocation_case,
    builtin_scan_help_precedence_cases, builtin_shared_help_precedence_cases,
    write_empty_manifest, BuiltinInvocationCase,
};

pub(super) fn assert_builtin_ok_for_empty_manifest(command: &str, cases: &[BuiltinInvocationCase]) {
    assert_builtin_ok_case_table_with_setup(command, cases, write_empty_manifest);
}

pub(super) fn assert_builtin_error_for_empty_manifest(
    command: &str,
    cases: &[BuiltinInvocationCase],
) {
    assert_builtin_error_case_table_with_setup(command, cases, write_empty_manifest);
}
