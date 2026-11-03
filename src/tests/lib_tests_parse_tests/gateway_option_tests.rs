use crate::tests::prelude::{
    parse_command, Command, GatewayArgs, GatewayPrivateArgs, GatewaySubcommand,
    InternalGatewayArgs, PathBuf,
};

/// One row per simple `gateway` subcommand mapping: status, up, down and
/// setup-tls with the local `--json` arm.
#[test]
fn parse_gateway_subcommands_map_their_flags() {
    let cases: &[(&[&str], GatewaySubcommand, bool)] = &[
        (
            &["gateway", "status", "--json"],
            GatewaySubcommand::Status,
            true,
        ),
        (&["gateway", "up"], GatewaySubcommand::Up, false),
        (&["gateway", "down"], GatewaySubcommand::Down, false),
        (
            &["gateway", "setup-tls", "--json"],
            GatewaySubcommand::SetupTls,
            true,
        ),
    ];
    for (args, subcommand, output_json) in cases {
        assert_eq!(
            parse_command(args.iter().map(|arg| (*arg).to_owned())).expect("parse should succeed"),
            Command::Gateway(GatewayArgs {
                subcommand: subcommand.clone(),
                private: None,
                output_json: *output_json,
            }),
            "{args:?}"
        );
    }
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
fn parse_gateway_repair_is_retired_with_its_replacement_named() {
    for args in [
        vec!["gateway", "repair"],
        vec!["gateway", "repair", "--yes"],
        vec![
            "gateway",
            "repair",
            "--owner-checkout",
            "/worktrees/deleted",
        ],
        vec!["gateway", "repair", "--yes", "--expect-plan", "abc123"],
    ] {
        let args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
        let error = parse_command(args).expect_err("`gateway repair` is retired in v0.15");
        let message = error.to_string();
        assert!(message.contains("retired in v0.15"), "{message}");
        assert!(message.contains("reclaimed automatically"), "{message}");
    }
}

#[test]
fn parse_gateway_retired_listener_handoff_commands_refuse_actionably() {
    for subcommand in [
        "setup-listener-handoff",
        "listener-handoff-status",
        "remove-listener-handoff",
    ] {
        let error = parse_command(vec!["gateway".to_owned(), subcommand.to_owned()])
            .expect_err("a retired listener-handoff command must refuse");
        let message = error.to_string();
        assert!(
            message.contains("retired") && message.contains("effigy gateway up"),
            "{subcommand} must name its replacement, got {message}"
        );
        assert!(
            message.contains("CHANGELOG.md"),
            "{subcommand} must point at the changelog entry, got {message}"
        );
    }
}

/// The internal `__gateway-run` surface: the handoff flag maps on and off.
#[test]
fn parse_internal_gateway_run_defaults_and_handoff_flag() {
    let cases: &[(&[&str], bool)] = &[
        (&["__gateway-run", "--listener-handoff"], true),
        (&["__gateway-run"], false),
    ];
    for (args, listener_handoff) in cases {
        assert_eq!(
            parse_command(args.iter().map(|arg| (*arg).to_owned())).expect("parse should succeed"),
            Command::InternalGateway(InternalGatewayArgs {
                private: None,
                listener_handoff: *listener_handoff,
            }),
            "{args:?}"
        );
    }
}

#[test]
fn parse_gateway_setup_accepts_explicit_public_suffixes() {
    let command = effigy_cli::parse_command(
        [
            "gateway",
            "setup",
            "--suffix",
            "dev.cumberland.co.uk",
            "--json",
        ]
        .map(String::from),
    )
    .unwrap();
    assert!(
        matches!(command, effigy_cli::Command::Gateway(effigy_cli::GatewayArgs { subcommand: effigy_cli::GatewaySubcommand::Setup { suffixes, remove: false }, output_json: true, .. }) if suffixes == ["dev.cumberland.co.uk"])
    );
}

#[test]
fn parse_gateway_setup_remove_is_explicit_and_rejects_private_mode() {
    let command =
        effigy_cli::parse_command(["gateway", "setup", "--remove"].map(String::from)).unwrap();
    assert!(matches!(
        command,
        effigy_cli::Command::Gateway(effigy_cli::GatewayArgs {
            subcommand: effigy_cli::GatewaySubcommand::Setup { remove: true, .. },
            ..
        })
    ));
    assert!(effigy_cli::parse_command(
        [
            "gateway",
            "setup",
            "--remove",
            "--private-state-root",
            "/unused"
        ]
        .map(String::from)
    )
    .is_err());
}
