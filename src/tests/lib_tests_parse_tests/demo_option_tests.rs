use super::prelude::{
    parse_command, Command, DemoArgs, DemoListGap, DemoListGroupBy, DemoListMode, DemoListQuery,
    DemoListStatus, DemoSubcommand, PathBuf,
};

#[test]
fn parse_demo_browser_with_grouping_and_repo() {
    let cmd = parse_command(vec![
        "demo".to_owned(),
        "browser".to_owned(),
        "--group-by".to_owned(),
        "status".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Demo(DemoArgs {
            subcommand: DemoSubcommand::Browser {
                group_by: Some(DemoListGroupBy::Status),
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: false,
        })
    );
}

#[test]
fn parse_demo_list_with_repo_and_json() {
    let cmd = parse_command(vec![
        "demo".to_owned(),
        "list".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Demo(DemoArgs {
            subcommand: DemoSubcommand::List {
                query: DemoListQuery::default(),
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_demo_list_with_filters_and_grouping() {
    let cmd = parse_command(vec![
        "demo".to_owned(),
        "list".to_owned(),
        "--search".to_owned(),
        "login".to_owned(),
        "--owner".to_owned(),
        "auth".to_owned(),
        "--tag".to_owned(),
        "smoke".to_owned(),
        "--mode".to_owned(),
        "interactive".to_owned(),
        "--cover".to_owned(),
        "auth.login".to_owned(),
        "--status".to_owned(),
        "ready".to_owned(),
        "--gap".to_owned(),
        "existing".to_owned(),
        "--stale-only".to_owned(),
        "--group-by".to_owned(),
        "owner".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Demo(DemoArgs {
            subcommand: DemoSubcommand::List {
                query: DemoListQuery {
                    search: Some("login".to_owned()),
                    owner: Some("auth".to_owned()),
                    tag: Some("smoke".to_owned()),
                    mode: Some(DemoListMode::Interactive),
                    cover: Some("auth.login".to_owned()),
                    status: Some(DemoListStatus::Ready),
                    gap: Some(DemoListGap::Existing),
                    stale_only: true,
                    group_by: Some(DemoListGroupBy::Owner),
                },
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_demo_inspect_with_repo_and_json() {
    let cmd = parse_command(vec![
        "demo".to_owned(),
        "inspect".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "login-smoke".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Demo(DemoArgs {
            subcommand: DemoSubcommand::Inspect {
                demo_id: "login-smoke".to_owned(),
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_demo_run_with_repo_and_json() {
    let cmd = parse_command(vec![
        "demo".to_owned(),
        "run".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "login-smoke".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Demo(DemoArgs {
            subcommand: DemoSubcommand::Run {
                demo_id: "login-smoke".to_owned(),
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_demo_stop_with_repo_and_json() {
    let cmd = parse_command(vec![
        "demo".to_owned(),
        "stop".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "login-smoke".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Demo(DemoArgs {
            subcommand: DemoSubcommand::Stop {
                demo_id: "login-smoke".to_owned(),
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_demo_rerun_with_repo_and_json() {
    let cmd = parse_command(vec![
        "demo".to_owned(),
        "rerun".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "login-smoke".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Demo(DemoArgs {
            subcommand: DemoSubcommand::Rerun {
                demo_id: "login-smoke".to_owned(),
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}
