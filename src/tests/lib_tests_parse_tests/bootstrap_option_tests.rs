use super::prelude::{parse_command, BootstrapArgs, Command, HelpTopic, PathBuf};

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
        "--start".to_owned(),
        "--plan".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Bootstrap(BootstrapArgs {
            repo_url: "git@github.com:inflatable-cookie/loophole.git".to_owned(),
            path: Some(PathBuf::from("./loophole")),
            branch: Some("main".to_owned()),
            start: true,
            plan: true,
            output_json: true,
        })
    );
}
