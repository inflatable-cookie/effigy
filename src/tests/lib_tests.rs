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
fn parse_catalogs_help_is_scoped() {
    let cmd = parse_command(vec!["catalogs".to_owned(), "--help".to_owned()])
        .expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Catalogs));
}

#[test]
fn parse_repo_pulse_help_is_scoped() {
    let cmd = parse_command(vec!["repo-pulse".to_owned(), "--help".to_owned()])
        .expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::RepoPulse));
}

#[test]
fn parse_help_command_alias_is_general_help() {
    let cmd = parse_command(vec!["help".to_owned()]).expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::General));
}

#[test]
fn parse_test_help_is_scoped() {
    let cmd =
        parse_command(vec!["test".to_owned(), "--help".to_owned()]).expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Test));
}

#[test]
fn render_help_writes_structured_sections() {
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false);
    render_help(&mut renderer, HelpTopic::General).expect("help render");
    let rendered = String::from_utf8(renderer.into_inner()).expect("utf8");
    assert!(rendered.contains("Commands"));
    assert!(rendered.contains("effigy help"));
    assert!(rendered.contains("Get Command Help"));
    assert!(rendered.contains("effigy config"));
    assert!(rendered.contains("effigy catalogs"));
    assert!(rendered.contains("effigy health"));
    assert!(rendered.contains("effigy test"));
    assert!(rendered.contains("<catalog>/test fallback"));
    assert!(rendered.contains("effigy test --plan"));
    assert!(rendered.contains("effigy test --help"));
    assert!(!rendered.contains("Quick Start"));
    assert!(!rendered.contains("effigy Help"));
}

#[test]
fn render_test_help_shows_detection_and_config() {
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false);
    render_help(&mut renderer, HelpTopic::Test).expect("help render");
    let rendered = String::from_utf8(renderer.into_inner()).expect("utf8");
    assert!(rendered.contains("test Help"));
    assert!(rendered.contains("<catalog>/test fallback"));
    assert!(rendered.contains("Detection Order"));
    assert!(rendered.contains("--verbose-results"));
    assert!(rendered.contains("--tui"));
    assert!(rendered.contains("[suite] [runner args]"));
    assert!(rendered.contains("effigy test vitest user-service"));
    assert!(rendered.contains("effigy farmyard/test"));
    assert!(rendered.contains("effigy test --plan user-service"));
    assert!(rendered.contains("effigy test --plan viteest user-service"));
    assert!(rendered.contains("Named Test Selection"));
    assert!(rendered.contains("effigy test user-service"));
    assert!(rendered.contains("prefix the suite explicitly"));
    assert!(rendered.contains("check `available-suites` per target"));
    assert!(rendered.contains("suggests nearest suite aliases"));
    assert!(rendered.contains("Migration"));
    assert!(rendered.contains("ambiguous in multi-suite repos"));
    assert!(rendered.contains("effigy test viteest user-service"));
    assert!(rendered.contains("suggests `effigy test vitest user-service`"));
    assert!(rendered.contains("effigy test nextest user_service --nocapture"));
    assert!(rendered.contains("Error Recovery"));
    assert!(rendered.contains("Ambiguity: `effigy test user-service`"));
    assert!(rendered.contains("Unavailable or mistyped suite"));
    assert!(rendered.contains("[package_manager]"));
    assert!(rendered.contains("js = \"pnpm\""));
    assert!(rendered.contains("[test]"));
    assert!(rendered.contains("max_parallel = 2"));
    assert!(rendered.contains("[test.runners]"));
    assert!(rendered.contains("vitest = \"pnpm exec vitest run\""));
    assert!(rendered.contains("[tasks.test]"));
}

#[test]
fn render_catalogs_help_shows_json_and_probe_options() {
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false);
    render_help(&mut renderer, HelpTopic::Catalogs).expect("help render");
    let rendered = String::from_utf8(renderer.into_inner()).expect("utf8");
    assert!(rendered.contains("catalogs Help"));
    assert!(rendered.contains("--resolve <SELECTOR>"));
    assert!(rendered.contains("--json"));
    assert!(rendered.contains("--pretty <true|false>"));
    assert!(rendered.contains("effigy catalogs --json --pretty false --resolve farmyard/api"));
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
