use super::prelude::*;

#[test]
fn parse_defaults_to_help_without_command() {
    let cmd = parse_command(Vec::<String>::new()).expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::General));
}

#[test]
fn parse_repo_pulse_is_treated_as_task_selector_after_builtin_removal() {
    let cmd = parse_command(vec![
        "repo-pulse".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Task(TaskInvocation {
            name: "repo-pulse".to_owned(),
            args: vec!["--repo".to_owned(), "/tmp/repo".to_owned()],
        })
    );
}

#[test]
fn parse_repo_pulse_help_flag_is_passthrough_after_builtin_removal() {
    let cmd = parse_command(vec!["repo-pulse".to_owned(), "--verbose-root".to_owned()])
        .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Task(TaskInvocation {
            name: "repo-pulse".to_owned(),
            args: vec!["--verbose-root".to_owned()],
        })
    );
}


#[test]
fn parse_runtime_task_passthrough() {
    let cmd = parse_command(vec![
        "snapshot".to_owned(),
        "--json".to_owned(),
        "--repo".to_owned(),
        ".".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Task(TaskInvocation {
            name: "snapshot".to_owned(),
            args: vec!["--json".to_owned(), "--repo".to_owned(), ".".to_owned()],
        })
    );
}


#[test]
fn parse_watch_passthrough_without_help() {
    let cmd = parse_command(vec!["watch".to_owned(), "services/api/dev".to_owned()])
        .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Task(TaskInvocation {
            name: "watch".to_owned(),
            args: vec!["services/api/dev".to_owned()],
        })
    );
}
