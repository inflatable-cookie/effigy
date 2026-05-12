use super::{
    container_exec_operation_from_options, is_runner_dispatch_feature, parse_rhai_embedded_command,
};
use effigy_cli::{Command, DocsArgs, DocsCheckKind, DocsSubcommand};
use effigy_rhai::surface::FEATURE_NAMES;
use serde_json::json;
use std::path::Path;
use std::{ffi::OsString, path::PathBuf};

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

#[test]
fn container_exec_operation_from_options_preserves_cwd_env_and_stdin_file() {
    let operation = container_exec_operation_from_options(
        Some("db"),
        &["mysql".to_owned(), "app".to_owned()],
        json!({
            "cwd": "/workspace/repo/db",
            "stdin_file": "/workspace/repo/input.sql",
            "env": {
                "MYSQL_PWD": "secret",
                "FOO": "bar"
            }
        }),
    )
    .expect("operation");

    assert_eq!(operation.service.as_deref(), Some("db"));
    assert_eq!(operation.command, vec!["mysql", "app"]);
    assert_eq!(operation.cwd, Some(PathBuf::from("/workspace/repo/db")));
    assert_eq!(
        operation.stdin_file,
        Some(PathBuf::from("/workspace/repo/input.sql"))
    );
    assert_eq!(
        operation.env.get("MYSQL_PWD"),
        Some(&OsString::from("secret"))
    );
    assert_eq!(operation.env.get("FOO"), Some(&OsString::from("bar")));
}
