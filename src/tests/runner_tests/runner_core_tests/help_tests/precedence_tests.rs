use super::prelude::{assert_builtin_help_case_table, BuiltinHelpCase};

#[test]
fn run_manifest_task_builtin_help_precedence_contract_table() {
    let cases = [
        BuiltinHelpCase {
            workspace: "builtin-cache-help-precedence",
            command: "cache",
            args: &["--wat", "--help"],
            expected: &["cache Help", "effigy cache inspect"],
        },
        BuiltinHelpCase {
            workspace: "builtin-completion-help-precedence",
            command: "completion",
            args: &["--help", "--wat"],
            expected: &["completion Help", "effigy completion <bash|zsh|fish>"],
        },
        BuiltinHelpCase {
            workspace: "builtin-completion-candidates-help-precedence",
            command: "completion",
            args: &["candidates", "--help", "--wat"],
            expected: &[
                "completion candidates Help",
                "effigy completion candidates [--repo <path>] [--prefix <value>] [--json]",
            ],
        },
        BuiltinHelpCase {
            workspace: "builtin-completion-candidates-help-json-precedence",
            command: "completion",
            args: &["candidates", "--help", "--json", "--wat"],
            expected: &[
                "\"schema\": \"effigy.help.v1\"",
                "\"topic\": \"completion-candidates\"",
            ],
        },
        BuiltinHelpCase {
            workspace: "builtin-config-help-precedence",
            command: "config",
            args: &["--help", "--wat"],
            expected: &["effigy.toml Reference", "[tasks]"],
        },
        BuiltinHelpCase {
            workspace: "builtin-scan-help-precedence",
            command: "scan",
            args: &["god-files", "--help", "--wat"],
            expected: &[
                "scan god-files Help",
                "effigy scan god-files [--threshold <N>] [--high <N>] [--critical <N>]",
                "effigy scan god-files [--show-warnings] [--no-gitignore]",
                "--show-warnings : include warning rows in terminal text output",
                "terminal text hides warning rows and prints a warning count summary",
            ],
        },
        BuiltinHelpCase {
            workspace: "builtin-scan-duplicate-help-precedence",
            command: "scan",
            args: &["duplicate-blocks", "--help", "--wat"],
            expected: &[
                "scan duplicate-blocks Help",
                "effigy scan duplicate-blocks [--threshold <N>] [--high <N>] [--critical <N>]",
                "effigy scan duplicate-blocks [--show-warnings] [--no-gitignore]",
                "--show-warnings : include warning rows in terminal text output",
                "terminal text hides warning rows and prints a warning count summary",
            ],
        },
        BuiltinHelpCase {
            workspace: "builtin-scan-comment-ratio-help-precedence",
            command: "scan",
            args: &["comment-ratio", "--help", "--wat"],
            expected: &[
                "scan comment-ratio Help",
                "effigy scan comment-ratio [--threshold <RATIO>] [--high <RATIO>] [--critical <RATIO>]",
                "effigy scan comment-ratio [--min-code-lines <N>] [--show-warnings] [--no-gitignore]",
                "--show-warnings : include warning rows in terminal text output",
                "terminal text hides warning rows and prints a warning count summary",
            ],
        },
        BuiltinHelpCase {
            workspace: "builtin-scan-attention-help-precedence",
            command: "scan",
            args: &["attention-markers", "--help", "--wat"],
            expected: &[
                "scan attention-markers Help",
                "effigy scan attention-markers [--show-warnings] [--no-gitignore]",
                "--show-warnings : include warning rows in terminal text output",
                "terminal text hides warning rows and prints a warning count summary",
            ],
        },
        BuiltinHelpCase {
            workspace: "builtin-unlock-help-precedence",
            command: "unlock",
            args: &["--wat", "--help"],
            expected: &["unlock Help", "effigy unlock [--all | <scope>...] [--json]"],
        },
    ];

    assert_builtin_help_case_table(&cases);
}
