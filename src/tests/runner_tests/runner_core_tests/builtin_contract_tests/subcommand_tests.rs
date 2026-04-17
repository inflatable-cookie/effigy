use crate::runner::tests::prelude::{
    assert_builtin_error_contract_case_table, assert_builtin_help_case_table,
    builtin_contract_error_case, builtin_scan_subcommand_help_cases,
    builtin_shared_help_precedence_cases,
};

#[test]
fn run_manifest_task_builtin_subcommand_error_contracts_are_stable() {
    let cases = [
        builtin_contract_error_case(
            "builtin-subcommand-cache-missing-subcommand",
            "cache",
            &[],
            &["`cache` requires a subcommand: `inspect` or `invalidate`"],
        ),
        builtin_contract_error_case(
            "builtin-subcommand-cache-unknown-subcommand",
            "cache",
            &["drop"],
            &["unknown cache subcommand `drop` (expected `inspect` or `invalidate`)"],
        ),
        builtin_contract_error_case(
            "builtin-subcommand-completion-unknown-shell",
            "completion",
            &["drop"],
            &[
                "invalid shell `drop` for `completion` (expected `bash`, `zsh`, `fish`, or `candidates`)",
            ],
        ),
        builtin_contract_error_case(
            "builtin-subcommand-completion-candidates-unknown-arg",
            "completion",
            &["candidates", "--wat"],
            &["unknown argument(s) for built-in `completion`: candidates --wat"],
        ),
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
    let mut cases = builtin_shared_help_precedence_cases(
        "builtin-subcommand-help-cache",
        "builtin-subcommand-help-completion",
        "builtin-subcommand-help-completion-candidates",
    );
    cases.extend(builtin_scan_subcommand_help_cases());

    assert_builtin_help_case_table(&cases);
}
