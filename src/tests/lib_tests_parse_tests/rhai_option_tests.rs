use crate::tests::prelude::{parse_command, Command, HelpTopic, RhaiArgs, RhaiSubcommand};

#[test]
fn parse_rhai_surface_accepts_json_flag() {
    let command = parse_command(vec![
        "rhai".to_owned(),
        "surface".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        command,
        Command::Rhai(RhaiArgs {
            subcommand: RhaiSubcommand::Surface,
            output_json: true,
        })
    );
}

#[test]
fn parse_rhai_help_is_scoped() {
    let command =
        parse_command(vec!["rhai".to_owned(), "--help".to_owned()]).expect("parse should succeed");

    assert_eq!(command, Command::Help(HelpTopic::Rhai));
}
