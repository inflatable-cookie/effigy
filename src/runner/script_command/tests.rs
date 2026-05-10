use super::{is_runner_dispatch_feature, parse_rhai_embedded_command};
use effigy_cli::{Command, DocsArgs, DocsCheckKind, DocsSubcommand};
use effigy_rhai::surface::FEATURE_NAMES;
use std::path::Path;

#[test]
fn parse_rhai_embedded_command_defaults_repo_override_when_missing() {
    let command = parse_rhai_embedded_command(
        Path::new("/tmp/repo"),
        &["docs".to_owned(), "check".to_owned(), "links".to_owned()],
        false,
    )
    .expect("parse rhai embedded command");

    assert!(matches!(
        command,
        Command::Docs(DocsArgs {
            subcommand: DocsSubcommand::Check {
                kind: DocsCheckKind::Links,
                ..
            },
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
            "check".to_owned(),
            "links".to_owned(),
            "--repo".to_owned(),
            "/tmp/other".to_owned(),
        ],
        false,
    )
    .expect("parse rhai embedded command");

    assert!(matches!(
        command,
        Command::Docs(DocsArgs {
            subcommand: DocsSubcommand::Check {
                kind: DocsCheckKind::Links,
                ..
            },
            repo_override: Some(path),
            output_json: false,
        }) if path == Path::new("/tmp/other")
    ));
}

#[test]
fn every_registered_rhai_feature_has_a_runner_dispatch_branch() {
    for feature in FEATURE_NAMES {
        assert!(
            is_runner_dispatch_feature(feature),
            "feature `{feature}` is registered in effigy-rhai but missing a runner dispatch branch"
        );
    }
}
