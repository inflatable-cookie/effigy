use super::{
    parse_command, render_cli_header, render_help, Command, HelpTopic, PulseArgs, TaskInvocation,
    TasksArgs,
};
use crate::ui::PlainRenderer;
use std::path::PathBuf;

#[test]
fn parse_defaults_to_help_without_command() {
    let cmd = parse_command(Vec::<String>::new()).expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::General));
}

#[test]
fn parse_repo_pulse_with_repo_override() {
    let cmd = parse_command(vec![
        "repo-pulse".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::RepoPulse(PulseArgs {
            repo_override: Some(PathBuf::from("/tmp/repo")),
            verbose_root: false,
        })
    );
}

#[test]
fn parse_repo_pulse_with_verbose_root() {
    let cmd = parse_command(vec!["repo-pulse".to_owned(), "--verbose-root".to_owned()])
        .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::RepoPulse(PulseArgs {
            repo_override: None,
            verbose_root: true,
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
fn parse_tasks_with_filters() {
    let cmd = parse_command(vec![
        "tasks".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--task".to_owned(),
        "reset-db".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Tasks(TasksArgs {
            repo_override: Some(PathBuf::from("/tmp/repo")),
            task_name: Some("reset-db".to_owned()),
        })
    );
}

#[test]
fn parse_tasks_help_is_scoped() {
    let cmd =
        parse_command(vec!["tasks".to_owned(), "--help".to_owned()]).expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Tasks));
}

#[test]
fn parse_repo_pulse_help_is_scoped() {
    let cmd = parse_command(vec!["repo-pulse".to_owned(), "--help".to_owned()])
        .expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::RepoPulse));
}

#[test]
fn render_help_writes_structured_sections() {
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false);
    render_help(&mut renderer, HelpTopic::General).expect("help render");
    let rendered = String::from_utf8(renderer.into_inner()).expect("utf8");
    assert!(rendered.contains("Quick Start"));
    assert!(rendered.contains("Commands"));
    assert!(rendered.contains("Get Command Help"));
}

#[test]
fn render_cli_header_includes_ascii_and_root() {
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false);
    render_cli_header(&mut renderer, PathBuf::from("/tmp/repo").as_path()).expect("header");
    let rendered = String::from_utf8(renderer.into_inner()).expect("utf8");
    assert!(rendered.contains("╭"));
    assert!(rendered.contains("EFFIGY"));
    assert!(rendered.contains("/tmp/repo"));
    assert!(rendered.contains(&format!("v{}", env!("CARGO_PKG_VERSION"))));
}
