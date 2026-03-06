use super::prelude::{
    assert_builtin_error_contract_case_table, assert_builtin_help_case_table,
    BuiltinContractErrorCase, BuiltinHelpCase,
};

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
        BuiltinContractErrorCase {
            workspace: "builtin-subcommand-scan-missing-subcommand",
            command: "scan",
            args: &[],
            expected: &["scan requires a subcommand (currently supported: `god-files`, `duplicate-blocks`, `comment-ratio`, `generated-assets`, `attention-markers`)"],
        },
        BuiltinContractErrorCase {
            workspace: "builtin-subcommand-scan-unknown-subcommand",
            command: "scan",
            args: &["wat"],
            expected: &["unknown argument(s) for built-in `scan`: wat"],
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
        BuiltinHelpCase {
            workspace: "builtin-subcommand-help-scan-god-files",
            command: "scan",
            args: &["god-files", "--help", "--wat"],
            expected: &[
                "scan god-files Help",
                "effigy scan god-files [--markdown] [--out reports/god-files.md]",
                "--show-warnings : include warning rows in terminal text output",
                "common docs, lockfiles, migrations, fixtures, examples, benchmarks, and generated artifacts are skipped by default",
            ],
        },
        BuiltinHelpCase {
            workspace: "builtin-subcommand-help-scan-duplicate-blocks",
            command: "scan",
            args: &["duplicate-blocks", "--help", "--wat"],
            expected: &[
                "scan duplicate-blocks Help",
                "effigy scan duplicate-blocks [--markdown] [--out reports/duplicate-blocks.md]",
                "--show-warnings : include warning rows in terminal text output",
                "detects repeated normalized code blocks across files, excluding common docs/data/generated paths by default",
            ],
        },
        BuiltinHelpCase {
            workspace: "builtin-subcommand-help-scan-comment-ratio",
            command: "scan",
            args: &["comment-ratio", "--help", "--wat"],
            expected: &[
                "scan comment-ratio Help",
                "effigy scan comment-ratio [--markdown] [--out reports/comment-ratio.md]",
                "--show-warnings : include warning rows in terminal text output",
                "counts comment-only lines against code-only lines in source and test files",
            ],
        },
        BuiltinHelpCase {
            workspace: "builtin-subcommand-help-scan-attention-markers",
            command: "scan",
            args: &["attention-markers", "--help", "--wat"],
            expected: &[
                "scan attention-markers Help",
                "effigy scan attention-markers [--markdown] [--out reports/attention-markers.md]",
                "--show-warnings : include warning rows in terminal text output",
                "detects TODO/FIXME/HACK/deprecation/workaround-style markers in source and test files",
            ],
        },
    ];

    assert_builtin_help_case_table(&cases);
}
