use super::prelude::{
    parse_command, strip_global_json_flag, strip_global_json_flags, Command, HelpTopic,
};

#[test]
fn strip_global_json_flag_removes_root_json_before_passthrough_delimiter() {
    let (args, json_mode) = strip_global_json_flag(vec![
        "tasks".to_owned(),
        "--json".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--".to_owned(),
        "--json".to_owned(),
    ]);
    assert!(json_mode);
    assert_eq!(
        args,
        vec![
            "tasks".to_owned(),
            "--repo".to_owned(),
            "/tmp/repo".to_owned(),
            "--".to_owned(),
            "--json".to_owned(),
        ]
    );
}

#[test]
fn strip_global_json_flags_supports_json() {
    let (args, json_mode) = strip_global_json_flags(vec![
        "tasks".to_owned(),
        "--json".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
    ]);
    assert!(json_mode);
    assert_eq!(
        args,
        vec![
            "tasks".to_owned(),
            "--repo".to_owned(),
            "/tmp/repo".to_owned(),
        ]
    );
}

#[test]
fn parse_command_rejects_unknown_global_flag_token() {
    let err = parse_command(vec!["--json-envelope".to_owned()]).expect_err("parse should fail");
    assert_eq!(err.to_string(), "unknown argument: --json-envelope");
}

#[test]
fn parse_command_rejects_removed_json_raw_flag_token() {
    let err = parse_command(vec!["--json-raw".to_owned()]).expect_err("parse should fail");
    assert_eq!(err.to_string(), "unknown argument: --json-raw");
}

#[test]
fn parse_tasks_help_is_scoped() {
    let cmd =
        parse_command(vec!["tasks".to_owned(), "--help".to_owned()]).expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Tasks));
}

#[test]
fn parse_catalogs_help_is_tasks_help_alias() {
    let cmd = parse_command(vec!["catalogs".to_owned(), "--help".to_owned()])
        .expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Tasks));
}

#[test]
fn parse_doctor_help_is_scoped() {
    let cmd = parse_command(vec!["doctor".to_owned(), "--help".to_owned()])
        .expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Doctor));
}

#[test]
fn parse_release_help_is_scoped() {
    let cmd = parse_command(vec!["release".to_owned(), "--help".to_owned()])
        .expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Release));
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
fn parse_watch_help_is_scoped() {
    let cmd =
        parse_command(vec!["watch".to_owned(), "--help".to_owned()]).expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Watch));
}

#[test]
fn parse_init_help_is_scoped() {
    let cmd =
        parse_command(vec!["init".to_owned(), "--help".to_owned()]).expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Init));
}

#[test]
fn parse_migrate_help_is_scoped() {
    let cmd = parse_command(vec!["migrate".to_owned(), "--help".to_owned()])
        .expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Migrate));
}
