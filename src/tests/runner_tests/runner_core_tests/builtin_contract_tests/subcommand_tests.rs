use super::prelude::*;

#[test]
fn run_manifest_task_builtin_subcommand_error_contracts_are_stable() {
    let cases = [
        BuiltinContractErrorCase {
            workspace: "builtin-subcommand-cache-missing-subcommand",
            command: "cache",
            args: &[],
            expected: &["`cache` requires a subcommand: `inspect` or `invalidate`"],
        },
        BuiltinContractErrorCase {
            workspace: "builtin-subcommand-cache-unknown-subcommand",
            command: "cache",
            args: &["drop"],
            expected: &["unknown cache subcommand `drop` (expected `inspect` or `invalidate`)"],
        },
        BuiltinContractErrorCase {
            workspace: "builtin-subcommand-completion-unknown-shell",
            command: "completion",
            args: &["drop"],
            expected: &[
                "invalid shell `drop` for `completion` (expected `bash`, `zsh`, `fish`, or `candidates`)",
            ],
        },
        BuiltinContractErrorCase {
            workspace: "builtin-subcommand-completion-candidates-unknown-arg",
            command: "completion",
            args: &["candidates", "--wat"],
            expected: &["unknown argument(s) for built-in `completion`: candidates --wat"],
        },
    ];

    assert_builtin_error_contract_case_table(&cases);
}

#[test]
fn run_manifest_task_builtin_subcommand_help_precedence_contracts_are_stable() {
    let cases = [
        BuiltinHelpCase {
            workspace: "builtin-subcommand-help-cache",
            command: "cache",
            args: &["--wat", "--help"],
            expected: &["cache Help", "effigy cache inspect"],
        },
        BuiltinHelpCase {
            workspace: "builtin-subcommand-help-completion",
            command: "completion",
            args: &["--help", "--wat"],
            expected: &["completion Help", "effigy completion <bash|zsh|fish>"],
        },
        BuiltinHelpCase {
            workspace: "builtin-subcommand-help-completion-candidates",
            command: "completion",
            args: &["candidates", "--help", "--wat"],
            expected: &[
                "completion candidates Help",
                "effigy completion candidates [--repo <path>] [--prefix <value>] [--json]",
            ],
        },
    ];

    assert_builtin_help_case_table(&cases);
}
