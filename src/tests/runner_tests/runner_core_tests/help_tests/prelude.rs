pub(super) use super::super::prelude::*;

pub(super) fn assert_builtin_ok_for_empty_manifest(command: &str, cases: &[BuiltinInvocationCase]) {
    assert_builtin_ok_case_table_with_setup(command, cases, write_empty_manifest);
}

pub(super) fn assert_builtin_error_for_empty_manifest(
    command: &str,
    cases: &[BuiltinInvocationCase],
) {
    assert_builtin_error_case_table_with_setup(command, cases, write_empty_manifest);
}
