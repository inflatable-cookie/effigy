use crate::runner::tests::prelude::{
    assert_builtin_error_contract_case_table, assert_builtin_help_case_table,
    builtin_contract_error_case, builtin_scan_subcommand_help_cases,
};

#[test]
fn run_manifest_task_builtin_subcommand_error_contracts_are_stable() {
    let cases = [
        builtin_contract_error_case(
            "builtin-subcommand-completion-unknown-shell",
            "completion",
            &["drop"],
            &[
                "invalid shell `drop` for `config completion` (expected `bash`, `zsh`, `fish`, or `candidates`)",
            ],
        ),
        // `completion candidates --wat` (shared unknown-argument case) lives in
        // argument_tests; `scan god-files --wat` likewise.
        builtin_contract_error_case(
            "builtin-subcommand-scan-unknown-subcommand",
            "scan",
            &["wat"],
            &["unknown argument(s) for built-in `scan`: wat"],
        ),
    ];

    assert_builtin_error_contract_case_table(&cases);
}

#[test]
fn run_manifest_task_builtin_subcommand_help_precedence_contracts_are_stable() {
    // The shared completion help-precedence rows are pinned by
    // help_tests/precedence_tests; this table owns the scan subcommand rows.
    let cases = builtin_scan_subcommand_help_cases();

    assert_builtin_help_case_table(&cases);
}
