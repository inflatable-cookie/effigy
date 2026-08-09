use crate::tests::prelude::{parse_command, Command};
use effigy_cli::{apply_global_cli_flags, GlobalCliOptions, PapercutsArgs, PapercutsSubcommand};
use std::path::{Path, PathBuf};

#[test]
fn bare_papercuts_parses_as_open_list() {
    assert_eq!(
        parse_command(["papercuts".to_owned()]).unwrap(),
        Command::Papercuts(PapercutsArgs {
            subcommand: PapercutsSubcommand::List {
                include_closed: false,
            },
            scope: None,
            output_json: false,
        })
    );
}

#[test]
fn papercuts_list_accepts_scope_all_and_json() {
    let parsed = parse_command([
        "papercuts".to_owned(),
        "--scope".to_owned(),
        "../projects".to_owned(),
        "--all".to_owned(),
        "--json".to_owned(),
    ])
    .unwrap();
    assert!(matches!(
        parsed,
        Command::Papercuts(PapercutsArgs {
            subcommand: PapercutsSubcommand::List { include_closed: true },
            scope: Some(scope),
            output_json: true,
        }) if scope == Path::new("../projects")
    ));
}

#[test]
fn papercuts_add_requires_and_captures_all_fields() {
    let parsed = parse_command([
        "papercuts".to_owned(),
        "add".to_owned(),
        "Noisy graph".to_owned(),
        "--friction".to_owned(),
        "large stale output".to_owned(),
        "--impact".to_owned(),
        "repeat cost".to_owned(),
        "--fix".to_owned(),
        "refresh once".to_owned(),
        "--surface".to_owned(),
        "Effigy graph".to_owned(),
    ])
    .unwrap();
    assert!(matches!(
        parsed,
        Command::Papercuts(PapercutsArgs {
            subcommand: PapercutsSubcommand::Add { title, friction, impact, possible_fix, surface },
            ..
        }) if title == "Noisy graph"
            && friction == "large stale output"
            && impact == "repeat cost"
            && possible_fix == "refresh once"
            && surface == "Effigy graph"
    ));

    let error = parse_command([
        "papercuts".to_owned(),
        "add".to_owned(),
        "Incomplete".to_owned(),
        "--friction".to_owned(),
        "f".to_owned(),
    ])
    .unwrap_err();
    assert!(error.to_string().contains("requires --impact <TEXT>"));
}

#[test]
fn global_json_applies_but_global_repo_is_rejected() {
    let json = apply_global_cli_flags(
        parse_command(["papercuts".to_owned()]).unwrap(),
        &GlobalCliOptions {
            json_mode: true,
            ..GlobalCliOptions::default()
        },
    )
    .unwrap();
    assert!(matches!(
        json,
        Command::Papercuts(PapercutsArgs {
            output_json: true,
            ..
        })
    ));

    let error = apply_global_cli_flags(
        parse_command(["papercuts".to_owned()]).unwrap(),
        &GlobalCliOptions {
            repo_override: Some(PathBuf::from("/tmp/project")),
            ..GlobalCliOptions::default()
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("--repo"));
}
