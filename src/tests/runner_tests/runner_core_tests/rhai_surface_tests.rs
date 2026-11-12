use crate::runner::run_command;
use crate::runner::tests::prelude::parse_json_output_with_schema_version;
use effigy_cli::{Command, RhaiArgs, RhaiSubcommand};

#[test]
fn run_manifest_task_builtin_rhai_surface_lists_git_helpers_in_text_and_json() {
    let text = run_command(Command::Rhai(RhaiArgs {
        subcommand: RhaiSubcommand::Surface,
        output_json: false,
    }))
    .expect("rhai surface text should render");
    assert!(text.lines().any(|line| line == "Rhai Surface"), "{text}");
    assert!(text.lines().any(|line| line == "git:"), "{text}");
    assert!(
        text.lines()
            .any(|line| line == "  - git::status() (read-only)"),
        "{text}"
    );

    let output = run_command(Command::Rhai(RhaiArgs {
        subcommand: RhaiSubcommand::Surface,
        output_json: true,
    }))
    .expect("rhai surface should render");

    let parsed = parse_json_output_with_schema_version(&output, "effigy.rhai.surface.v1", 1);
    assert!(parsed["modules"]
        .as_array()
        .expect("modules")
        .iter()
        .any(|module| module.as_str() == Some("git")));
    assert!(parsed["functions"]
        .as_array()
        .expect("functions")
        .iter()
        .any(|function| function["module"] == "git" && function["name"] == "status"));
}
