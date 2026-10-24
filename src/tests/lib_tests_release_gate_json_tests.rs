use super::prelude::{parse_command, Command, ReleaseArgs, ReleaseSubcommand};

#[test]
fn parse_json_release_gates_keeps_output_json_without_new_flags() {
    let command = parse_command(vec![
        "--json".to_owned(),
        "release".to_owned(),
        "gates".to_owned(),
    ])
    .expect("parse should succeed");

    assert!(matches!(
        command,
        Command::Release(ReleaseArgs {
            subcommand: ReleaseSubcommand::Gates,
            output_json: true,
            ..
        })
    ));
}

/// The JSON release commands that keep their schema (contract 054 §5): the Run
/// that replaces `prepare`/`execute` still answers `--json`.
#[test]
fn parse_json_release_status_prepare_and_execute_keep_existing_schema_commands() {
    for args in [
        vec![
            "--json".to_owned(),
            "release".to_owned(),
            "status".to_owned(),
        ],
        vec![
            "--json".to_owned(),
            "release".to_owned(),
            "run".to_owned(),
            "--to".to_owned(),
            "certify".to_owned(),
        ],
        vec![
            "--json".to_owned(),
            "release".to_owned(),
            "resume".to_owned(),
            "run-17".to_owned(),
        ],
    ] {
        let command = parse_command(args.clone()).expect("parse should succeed");
        match command {
            Command::Release(ReleaseArgs {
                output_json: true, ..
            }) => {}
            other => panic!("expected json release command for {args:?}, got: {other:?}"),
        }
    }
}

/// v0.15 retired `release prepare` and `release execute` into one durable Run
/// (`effigy release run`, resumed by `effigy release resume <run>`; #269). The
/// parser refuses both by name and points at the replacement.
#[test]
fn parse_json_release_prepare_and_execute_are_retired_into_the_run() {
    for retired in ["prepare", "execute"] {
        let error = parse_command(vec![
            "--json".to_owned(),
            "release".to_owned(),
            retired.to_owned(),
            "--plan".to_owned(),
        ])
        .expect_err("a retired release subcommand is refused");
        let message = error.to_string();
        assert!(
            message.contains(&format!("`effigy release {retired}` is retired in v0.15")),
            "{message}"
        );
        assert!(message.contains("effigy release run"), "{message}");
        assert!(
            message.contains("v0.15.0 Removed entry in CHANGELOG.md"),
            "{message}"
        );
    }
}
