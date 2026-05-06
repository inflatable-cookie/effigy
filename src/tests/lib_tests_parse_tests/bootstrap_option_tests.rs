use crate::tests::prelude::{
    parse_command, BootstrapArgs, BootstrapDepsSyncMode, BootstrapSubcommand, Command, HelpTopic,
    PathBuf,
};
use effigy_cli::BootstrapDbSeedInput;

#[test]
fn parse_bootstrap_help_is_scoped() {
    let cmd = parse_command(vec!["bootstrap".to_owned(), "--help".to_owned()])
        .expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Bootstrap));
}

#[test]
fn parse_bootstrap_plan_with_path_branch_and_start() {
    let cmd = parse_command(vec![
        "bootstrap".to_owned(),
        "git@github.com:inflatable-cookie/loophole.git".to_owned(),
        "--path".to_owned(),
        "./loophole".to_owned(),
        "--branch".to_owned(),
        "main".to_owned(),
        "--db-seed".to_owned(),
        "./infra/bootstrap/latest.sql".to_owned(),
        "--start".to_owned(),
        "--plan".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Bootstrap(BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: "git@github.com:inflatable-cookie/loophole.git".to_owned(),
                path: Some(PathBuf::from("./loophole")),
                branch: Some("main".to_owned()),
                db_seeds: vec![BootstrapDbSeedInput {
                    target: None,
                    path: PathBuf::from("./infra/bootstrap/latest.sql"),
                }],
                fresh: false,
                no_prompt: false,
                reuse_path: false,
                start: true,
                plan: true,
            },
            output_json: true,
        })
    );
}

#[test]
fn parse_bootstrap_defaults_to_start_when_unspecified() {
    let cmd = parse_command(vec![
        "bootstrap".to_owned(),
        "git@github.com:inflatable-cookie/loophole.git".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Bootstrap(BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: "git@github.com:inflatable-cookie/loophole.git".to_owned(),
                path: None,
                branch: None,
                db_seeds: Vec::new(),
                fresh: false,
                no_prompt: false,
                reuse_path: false,
                start: true,
                plan: false,
            },
            output_json: false,
        })
    );
}

#[test]
fn parse_bootstrap_no_start_disables_default_start() {
    let cmd = parse_command(vec![
        "bootstrap".to_owned(),
        "git@github.com:inflatable-cookie/loophole.git".to_owned(),
        "--no-start".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Bootstrap(BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: "git@github.com:inflatable-cookie/loophole.git".to_owned(),
                path: None,
                branch: None,
                db_seeds: Vec::new(),
                fresh: false,
                no_prompt: false,
                reuse_path: false,
                start: false,
                plan: false,
            },
            output_json: false,
        })
    );
}

#[test]
fn parse_bootstrap_accepts_repeated_db_seed_flags() {
    let cmd = parse_command(vec![
        "bootstrap".to_owned(),
        "git@github.com:inflatable-cookie/loophole.git".to_owned(),
        "--db-seed".to_owned(),
        "./db/latest.sql".to_owned(),
        "--db-seed".to_owned(),
        "./db/legacy.sql".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Bootstrap(BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: "git@github.com:inflatable-cookie/loophole.git".to_owned(),
                path: None,
                branch: None,
                db_seeds: vec![
                    BootstrapDbSeedInput {
                        target: None,
                        path: PathBuf::from("./db/latest.sql"),
                    },
                    BootstrapDbSeedInput {
                        target: None,
                        path: PathBuf::from("./db/legacy.sql"),
                    },
                ],
                fresh: false,
                no_prompt: false,
                reuse_path: false,
                start: true,
                plan: false,
            },
            output_json: false,
        })
    );
}

#[test]
fn parse_bootstrap_accepts_bare_target_db_seed_flag() {
    let cmd = parse_command(vec![
        "bootstrap".to_owned(),
        "git@github.com:inflatable-cookie/loophole.git".to_owned(),
        "--db-seed".to_owned(),
        "legacy_mysql".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Bootstrap(BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: "git@github.com:inflatable-cookie/loophole.git".to_owned(),
                path: None,
                branch: None,
                db_seeds: vec![BootstrapDbSeedInput {
                    target: Some("legacy_mysql".to_owned()),
                    path: PathBuf::from("legacy_mysql.sql"),
                }],
                fresh: false,
                no_prompt: false,
                reuse_path: false,
                start: true,
                plan: false,
            },
            output_json: false,
        })
    );
}

#[test]
fn parse_bootstrap_accepts_named_db_seed_flags() {
    let cmd = parse_command(vec![
        "bootstrap".to_owned(),
        "git@github.com:inflatable-cookie/loophole.git".to_owned(),
        "--db-seed".to_owned(),
        "cbs=./db/cbs.sql".to_owned(),
        "--db-seed".to_owned(),
        "cbs-mortcalc=./db/mortcalc.sql".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Bootstrap(BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: "git@github.com:inflatable-cookie/loophole.git".to_owned(),
                path: None,
                branch: None,
                db_seeds: vec![
                    BootstrapDbSeedInput {
                        target: Some("cbs".to_owned()),
                        path: PathBuf::from("./db/cbs.sql"),
                    },
                    BootstrapDbSeedInput {
                        target: Some("cbs-mortcalc".to_owned()),
                        path: PathBuf::from("./db/mortcalc.sql"),
                    },
                ],
                fresh: false,
                no_prompt: false,
                reuse_path: false,
                start: true,
                plan: false,
            },
            output_json: false,
        })
    );
}

#[test]
fn parse_bootstrap_accepts_no_prompt_flag() {
    let cmd = parse_command(vec![
        "bootstrap".to_owned(),
        "git@github.com:inflatable-cookie/loophole.git".to_owned(),
        "--no-prompt".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Bootstrap(BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: "git@github.com:inflatable-cookie/loophole.git".to_owned(),
                path: None,
                branch: None,
                db_seeds: Vec::new(),
                fresh: false,
                no_prompt: true,
                reuse_path: false,
                start: true,
                plan: false,
            },
            output_json: false,
        })
    );
}

#[test]
fn parse_bootstrap_accepts_reuse_path_flag() {
    let cmd = parse_command(vec![
        "bootstrap".to_owned(),
        "git@github.com:inflatable-cookie/loophole.git".to_owned(),
        "--reuse-path".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Bootstrap(BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: "git@github.com:inflatable-cookie/loophole.git".to_owned(),
                path: None,
                branch: None,
                db_seeds: Vec::new(),
                fresh: false,
                no_prompt: false,
                reuse_path: true,
                start: true,
                plan: false,
            },
            output_json: false,
        })
    );
}

#[test]
fn parse_bootstrap_accepts_fresh_flag() {
    let cmd = parse_command(vec![
        "bootstrap".to_owned(),
        "git@github.com:inflatable-cookie/loophole.git".to_owned(),
        "--fresh".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Bootstrap(BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: "git@github.com:inflatable-cookie/loophole.git".to_owned(),
                path: None,
                branch: None,
                db_seeds: Vec::new(),
                fresh: true,
                no_prompt: false,
                reuse_path: false,
                start: true,
                plan: false,
            },
            output_json: false,
        })
    );
}

#[test]
fn parse_bootstrap_teardown_subcommand() {
    let cmd = parse_command(vec![
        "bootstrap".to_owned(),
        "teardown".to_owned(),
        "--yes".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Bootstrap(BootstrapArgs {
            subcommand: BootstrapSubcommand::Teardown { yes: true },
            output_json: true,
        })
    );
}

#[test]
fn parse_bootstrap_deps_sync_subcommand() {
    let cmd = parse_command(vec![
        "bootstrap".to_owned(),
        "deps".to_owned(),
        "sync".to_owned(),
        "../underlay".to_owned(),
        "api".to_owned(),
        "--rust-only".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Bootstrap(BootstrapArgs {
            subcommand: BootstrapSubcommand::DepsSync {
                mode: BootstrapDepsSyncMode::RustOnly,
                paths: vec!["../underlay".to_owned(), "api".to_owned()],
            },
            output_json: true,
        })
    );
}
