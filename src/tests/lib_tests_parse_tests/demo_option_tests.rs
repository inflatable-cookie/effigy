use super::prelude::{parse_command, Command, DemoArgs, DemoSubcommand, PathBuf};

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
            subcommand: DemoSubcommand::List,
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
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
