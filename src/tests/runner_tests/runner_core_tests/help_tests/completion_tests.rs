use crate::runner::tests::prelude::{
    assert_builtin_error_for_empty_manifest, assert_builtin_ok_for_empty_manifest,
    builtin_invocation_case, run_builtin_ok, temp_workspace, write_root_manifest,
};
use effigy_ui::PlainRenderer;

#[test]
fn run_manifest_task_builtin_help_topics_render_expected_content() {
    let init_root = temp_workspace("builtin-init-help");
    write_root_manifest(&init_root, "");
    let init = run_builtin_ok(init_root, "init", &["--help"]);
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false);
    effigy_cli::help::ui::render_help(&mut renderer, effigy_cli::HelpTopic::Init)
        .expect("render typed init help panel");
    let expected_init = String::from_utf8(renderer.into_inner()).expect("utf8 init help");
    assert_eq!(init, expected_init);

    let completion_root = temp_workspace("builtin-completion-help");
    write_root_manifest(&completion_root, "");
    let completion = run_builtin_ok(completion_root, "completion", &["--help"]);
    for line in [
        "completion Help",
        "Usage",
        "effigy config completion [<bash|zsh|fish>] [--install|--export] [--json]",
        "effigy config completion candidates [--repo <path>] [--prefix <value>] [--json]",
    ] {
        assert!(
            completion.lines().any(|actual| actual == line),
            "missing {line:?}: {completion}"
        );
    }
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
