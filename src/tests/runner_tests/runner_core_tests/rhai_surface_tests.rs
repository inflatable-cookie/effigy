use crate::runner::entrypoints::run_command;
use crate::runner::tests::prelude::parse_json_output_with_schema_version;
use effigy_cli::{Command, RhaiArgs, RhaiSubcommand};

#[test]
fn run_manifest_task_builtin_rhai_surface_json_lists_git_helpers() {
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

#[test]
fn run_manifest_task_builtin_rhai_surface_text_lists_git_module() {
    let output = run_command(Command::Rhai(RhaiArgs {
        subcommand: RhaiSubcommand::Surface,
        output_json: false,
    }))
    .expect("rhai surface should render");

    assert!(output.contains("Rhai Surface"));
    assert!(output.contains("git:"));
    assert!(output.contains("git::status()"));
}
