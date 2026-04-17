use crate::runner::tests::prelude::{
    assert_builtin_error_for_empty_manifest, assert_builtin_help_case_table,
    assert_builtin_ok_for_empty_manifest, builtin_help_case, builtin_invocation_case,
};

#[test]
fn run_manifest_task_builtin_help_topics_render_expected_content() {
    let cases = [
        builtin_help_case(
            "builtin-init-help",
            "init",
            &["--help"],
            &["init Help", "effigy init [--dry-run] [--force] [--json]"],
        ),
        builtin_help_case(
            "builtin-migrate-help-json",
            "migrate",
            &["--help", "--json"],
            &["\"schema\": \"effigy.help.v1\"", "\"topic\": \"migrate\""],
        ),
        builtin_help_case(
            "builtin-completion-help",
            "completion",
            &["--help"],
            &[
                "completion Help",
                "effigy completion <bash|zsh|fish> [--json]",
            ],
        ),
    ];

    assert_builtin_help_case_table(&cases);
}

#[test]
fn run_manifest_task_builtin_completion_ok_contract_table() {
    let cases = [
        builtin_invocation_case(
            "builtin-completion-bash",
            &["bash"],
            &["complete -F _effigy effigy", "cache completion"],
        ),
        builtin_invocation_case(
            "builtin-completion-json",
            &["zsh", "--json"],
            &[
                "\"schema\": \"effigy.completion.v1\"",
                "\"shell\": \"zsh\"",
                "\"commands\"",
            ],
        ),
    ];

    assert_builtin_ok_for_empty_manifest("completion", &cases);
}

#[test]
fn run_manifest_task_builtin_completion_argument_validation_table() {
    let cases = [
        builtin_invocation_case(
            "builtin-completion-shell-required",
            &[],
            &["`completion` requires a shell target (`bash`, `zsh`, or `fish`)"],
        ),
        builtin_invocation_case(
            "builtin-completion-multiple-shell-targets",
            &["bash", "zsh"],
            &["`completion` accepts exactly one shell target (`bash`, `zsh`, or `fish`)"],
        ),
        builtin_invocation_case(
            "builtin-completion-candidates-missing-prefix-value",
            &["candidates", "--prefix"],
            &["completion candidates argument --prefix requires a value"],
        ),
    ];

    assert_builtin_error_for_empty_manifest("completion", &cases);
}
