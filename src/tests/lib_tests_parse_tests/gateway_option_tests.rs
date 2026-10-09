use crate::tests::prelude::{
    parse_command, Command, GatewayArgs, GatewayPrivateArgs, GatewaySubcommand, HelpTopic, PathBuf,
};

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
            private: None,
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
            private: None,
            output_json: false,
        })
    );
    assert_eq!(
        down,
        Command::Gateway(GatewayArgs {
            subcommand: GatewaySubcommand::Down,
            private: None,
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
            private: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_gateway_private_startup_and_tls_options() {
    let up = parse_command(vec![
        "gateway".to_owned(),
        "up".to_owned(),
        "--private-state-root".to_owned(),
        "/tmp/effigy-gateway-private".to_owned(),
        "--dns-addr".to_owned(),
        "127.0.0.1:0".to_owned(),
        "--proxy-addr".to_owned(),
        "127.0.0.1:0".to_owned(),
        "--https-addr".to_owned(),
        "127.0.0.1:0".to_owned(),
        "--json".to_owned(),
    ])
    .expect("private gateway up options");
    assert_eq!(
        up,
        Command::Gateway(GatewayArgs {
            subcommand: GatewaySubcommand::Up,
            private: Some(GatewayPrivateArgs {
                state_root: PathBuf::from("/tmp/effigy-gateway-private"),
                dns_addr: Some("127.0.0.1:0".parse().unwrap()),
                proxy_addr: Some("127.0.0.1:0".parse().unwrap()),
                https_addr: Some("127.0.0.1:0".parse().unwrap()),
            }),
            output_json: true,
        })
    );

    let setup_tls = parse_command(vec![
        "gateway".to_owned(),
        "setup-tls".to_owned(),
        "--private-state-root".to_owned(),
        "/tmp/effigy-gateway-private".to_owned(),
    ])
    .expect("private TLS setup");
    assert!(matches!(
        setup_tls,
        Command::Gateway(GatewayArgs {
            subcommand: GatewaySubcommand::SetupTls,
            private: Some(_),
            ..
        })
    ));
}

#[test]
fn parse_gateway_bind_overrides_require_explicit_private_root() {
    let error = parse_command(vec![
        "gateway".to_owned(),
        "up".to_owned(),
        "--dns-addr".to_owned(),
        "127.0.0.1:0".to_owned(),
    ])
    .expect_err("bind override must be private");
    assert!(error.to_string().contains("require `--private-state-root`"));
}

#[test]
fn parse_gateway_repair_accepts_yes_and_json() {
    let cmd = parse_command(vec![
        "gateway".to_owned(),
        "repair".to_owned(),
        "--yes".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse");

    assert_eq!(
        cmd,
        Command::Gateway(GatewayArgs {
            subcommand: GatewaySubcommand::Repair { yes: true },
            private: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_gateway_recover_accepts_adopt_candidate_and_json() {
    let cmd = parse_command(vec![
        "gateway".to_owned(),
        "recover".to_owned(),
        "--adopt-candidate".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse recover");
    assert_eq!(
        cmd,
        Command::Gateway(GatewayArgs {
            subcommand: GatewaySubcommand::Recover {
                yes: false,
                adopt_candidate: true,
            },
            private: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_gateway_recover_yes_does_not_imply_adopt() {
    let cmd = parse_command(vec![
        "gateway".to_owned(),
        "recover".to_owned(),
        "--yes".to_owned(),
    ])
    .expect("parse recover --yes");
    assert_eq!(
        cmd,
        Command::Gateway(GatewayArgs {
            subcommand: GatewaySubcommand::Recover {
                yes: true,
                adopt_candidate: false,
            },
            private: None,
            output_json: false,
        })
    );
}
