
use super::parse_rhai_embedded_command;
use effigy_cli::{Command, DocsArgs, DocsSubcommand};
use std::path::Path;

#[test]
fn parse_rhai_embedded_command_defaults_repo_override_when_missing() {
    let command = parse_rhai_embedded_command(
        Path::new("/tmp/repo"),
        &["docs".to_owned(), "check-links".to_owned()],
        false,
    )
    .expect("parse rhai embedded command");

    assert!(matches!(
        command,
        Command::Docs(DocsArgs {
            subcommand: DocsSubcommand::CheckLinks { .. },
            repo_override: Some(path),
            output_json: false,
        }) if path == Path::new("/tmp/repo")
    ));
}

#[test]
fn parse_rhai_embedded_command_preserves_explicit_repo_override() {
    let command = parse_rhai_embedded_command(
        Path::new("/tmp/repo"),
        &[
            "docs".to_owned(),
            "check-links".to_owned(),
            "--repo".to_owned(),
            "/tmp/other".to_owned(),
        ],
        false,
    )
    .expect("parse rhai embedded command");

    assert!(matches!(
        command,
        Command::Docs(DocsArgs {
            subcommand: DocsSubcommand::CheckLinks { .. },
            repo_override: Some(path),
            output_json: false,
        }) if path == Path::new("/tmp/other")
    ));
}
