use super::prelude::*;

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

    for case in cases {
        assert_builtin_help_case(&case);
    }
}

#[test]
fn run_manifest_task_builtin_completion_bash_outputs_script() {
    let root = temp_workspace("builtin-completion-bash");
    write_empty_manifest(&root);

    assert_builtin_ok_contains(
        root,
        "completion",
        &["bash"],
        &["complete -F _effigy effigy", "cache completion"],
    );
}

#[test]
fn run_manifest_task_builtin_completion_json_uses_completion_schema() {
    let root = temp_workspace("builtin-completion-json");
    write_empty_manifest(&root);

    assert_builtin_ok_contains(
        root,
        "completion",
        &["zsh", "--json"],
        &[
            "\"schema\": \"effigy.completion.v1\"",
            "\"shell\": \"zsh\"",
            "\"commands\"",
        ],
    );
}
