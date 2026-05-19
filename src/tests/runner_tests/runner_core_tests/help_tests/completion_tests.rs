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
            &[
                "init Help",
                "effigy init [--check|--apply|--repair] [--json]",
                "effigy init --checklist [--json]",
                "effigy init --apply-actions <ID>[,<ID>...] [--json]",
                "effigy init <name> [--dry-run] [--force] [--json]",
                "effigy init --list [--json]",
            ],
        ),
        builtin_help_case(
            "builtin-migrate-help-json",
            "migrate",
            &["--help", "--json"],
            &[
                "\"schema\": \"effigy.help.v1\"",
                "\"topic\": \"tasks-migrate\"",
            ],
        ),
        builtin_help_case(
            "builtin-completion-help",
            "completion",
            &["--help"],
            &[
                "completion Help",
                "effigy config completion [<bash|zsh|fish>] [--install|--export] [--json]",
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
            &["bash", "--export"],
            &[
                "complete -F _effigy effigy",
                "effigy config completion candidates --prefix \"$cur\"",
            ],
        ),
        builtin_invocation_case(
            "builtin-completion-json",
            &["zsh", "--export", "--json"],
            &[
                "\"schema\": \"effigy.completion.v2\"",
                "\"shell\": \"zsh\"",
                "\"action\": \"export\"",
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
            &["`config completion` requires a shell target (`bash`, `zsh`, or `fish`) when prompting is unavailable"],
        ),
        builtin_invocation_case(
            "builtin-completion-multiple-shell-targets",
            &["bash", "zsh"],
            &["`config completion` accepts exactly one shell target (`bash`, `zsh`, or `fish`)"],
        ),
        builtin_invocation_case(
            "builtin-completion-action-required",
            &["bash"],
            &["`config completion` requires an action (`--install` or `--export`) when prompting is unavailable"],
        ),
        builtin_invocation_case(
            "builtin-completion-conflicting-actions",
            &["bash", "--install", "--export"],
            &["`config completion` accepts exactly one completion action (`--install` or `--export`)"],
        ),
        builtin_invocation_case(
            "builtin-completion-candidates-missing-prefix-value",
            &["candidates", "--prefix"],
            &["completion candidates argument --prefix requires a value"],
        ),
    ];

    assert_builtin_error_for_empty_manifest("completion", &cases);
}
