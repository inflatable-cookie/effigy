use crate::tests::prelude::{parse_command, Command, GatewayArgs, GatewaySubcommand, HelpTopic};

#[test]
fn parse_gateway_help_is_scoped() {
    let cmd = parse_command(vec!["gateway".to_owned(), "--help".to_owned()])
        .expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Gateway));
}

#[test]
fn parse_gateway_status_supports_json() {
    let cmd = parse_command(vec![
        "gateway".to_owned(),
        "status".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Gateway(GatewayArgs {
            subcommand: GatewaySubcommand::Status,
            output_json: true,
        })
    );
}

#[test]
fn parse_gateway_up_and_down_commands() {
    let up = parse_command(vec!["gateway".to_owned(), "up".to_owned()]).expect("up");
    let down = parse_command(vec!["gateway".to_owned(), "down".to_owned()]).expect("down");

    assert_eq!(
        up,
        Command::Gateway(GatewayArgs {
            subcommand: GatewaySubcommand::Up,
            output_json: false,
        })
    );
    assert_eq!(
        down,
        Command::Gateway(GatewayArgs {
            subcommand: GatewaySubcommand::Down,
            output_json: false,
        })
    );
}

#[test]
fn parse_gateway_setup_tls_supports_json() {
    let cmd = parse_command(vec![
        "gateway".to_owned(),
        "setup-tls".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Gateway(GatewayArgs {
            subcommand: GatewaySubcommand::SetupTls,
            output_json: true,
        })
    );
}
