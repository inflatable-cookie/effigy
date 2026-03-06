use super::prelude::{
    assert_builtin_error_for_empty_manifest, assert_builtin_help_case_table,
    assert_builtin_ok_for_empty_manifest, BuiltinHelpCase, BuiltinInvocationCase,
};

#[test]
fn run_manifest_task_builtin_help_topics_render_expected_content() {
    let cases = [
        BuiltinHelpCase {
            workspace: "builtin-init-help",
            command: "init",
            args: &["--help"],
            expected: &["init Help", "effigy init [--dry-run] [--force] [--json]"],
        },
        BuiltinHelpCase {
            workspace: "builtin-migrate-help-json",
            command: "migrate",
            args: &["--help", "--json"],
            expected: &["\"schema\": \"effigy.help.v1\"", "\"topic\": \"migrate\""],
        },
        BuiltinHelpCase {
            workspace: "builtin-completion-help",
            command: "completion",
            args: &["--help"],
            expected: &[
                "completion Help",
                "effigy completion <bash|zsh|fish> [--json]",
            ],
        },
    ];

    assert_builtin_help_case_table(&cases);
}

#[test]
fn run_manifest_task_builtin_completion_ok_contract_table() {
    let cases = [
        BuiltinInvocationCase {
            workspace: "builtin-completion-bash",
            args: &["bash"],
            expected: &["complete -F _effigy effigy", "cache completion"],
        },
        BuiltinInvocationCase {
            workspace: "builtin-completion-json",
            args: &["zsh", "--json"],
            expected: &[
                "\"schema\": \"effigy.completion.v1\"",
                "\"shell\": \"zsh\"",
                "\"commands\"",
            ],
        },
    ];

    assert_builtin_ok_for_empty_manifest("completion", &cases);
}

#[test]
fn run_manifest_task_builtin_completion_argument_validation_table() {
    let cases = [
        BuiltinInvocationCase {
            workspace: "builtin-completion-shell-required",
            args: &[],
            expected: &["`completion` requires a shell target (`bash`, `zsh`, or `fish`)"],
        },
        BuiltinInvocationCase {
            workspace: "builtin-completion-multiple-shell-targets",
            args: &["bash", "zsh"],
            expected: &["`completion` accepts exactly one shell target (`bash`, `zsh`, or `fish`)"],
        },
        BuiltinInvocationCase {
            workspace: "builtin-completion-candidates-missing-prefix-value",
            args: &["candidates", "--prefix"],
            expected: &["completion candidates argument --prefix requires a value"],
        },
    ];

    assert_builtin_error_for_empty_manifest("completion", &cases);
}
