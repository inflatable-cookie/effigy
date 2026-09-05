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
            "prepare".to_owned(),
            "--plan".to_owned(),
            "--check-gates".to_owned(),
        ],
        vec![
            "--json".to_owned(),
            "release".to_owned(),
            "execute".to_owned(),
            "--plan".to_owned(),
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
