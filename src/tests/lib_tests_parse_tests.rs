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
fn parse_doctor_with_repo_fix_and_json() {
    let cmd = parse_command(vec![
        "doctor".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--fix".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Doctor(DoctorArgs {
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
            fix: true,
            verbose: false,
            explain: None,
        })
    );
}

#[test]
fn parse_doctor_with_verbose_flag() {
    let cmd = parse_command(vec!["doctor".to_owned(), "--verbose".to_owned()])
        .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Doctor(DoctorArgs {
            repo_override: None,
            output_json: false,
            fix: false,
            verbose: true,
            explain: None,
        })
    );
}

#[test]
fn parse_doctor_with_explain_target_and_args() {
    let cmd = parse_command(vec![
        "doctor".to_owned(),
        "farmyard/build".to_owned(),
        "--".to_owned(),
        "--watch".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Doctor(DoctorArgs {
            repo_override: None,
            output_json: false,
            fix: false,
            verbose: false,
            explain: Some(TaskInvocation {
                name: "farmyard/build".to_owned(),
                args: vec!["--".to_owned(), "--watch".to_owned()],
            }),
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
fn parse_tasks_with_filters() {
    let cmd = parse_command(vec![
        "tasks".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--task".to_owned(),
        "db:reset".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Tasks(TasksArgs {
            repo_override: Some(PathBuf::from("/tmp/repo")),
            task_name: Some("db:reset".to_owned()),
            resolve_selector: None,
            output_json: false,
            pretty_json: true,
        })
    );
}

#[test]
fn parse_tasks_supports_json_flag() {
    let cmd =
        parse_command(vec!["tasks".to_owned(), "--json".to_owned()]).expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Tasks(TasksArgs {
            repo_override: None,
            task_name: None,
            resolve_selector: None,
            output_json: true,
            pretty_json: true,
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
