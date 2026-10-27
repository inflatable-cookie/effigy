use crate::tests::prelude::{parse_command, Command, RhaiArgs, RhaiSubcommand};

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