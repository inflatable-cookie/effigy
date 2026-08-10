use crate::tests::prelude::{
    parse_command, strip_global_json_flag, strip_global_json_flags, Command, DoctorArgs, HelpTopic,
    PathBuf, TaskInvocation, TasksArgs,
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
fn parse_command_applies_leading_repo_to_doctor() {
    let cmd = parse_command(vec![
        "--repo".to_owned(),
        "/tmp/workspace".to_owned(),
        "doctor".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Doctor(DoctorArgs {
            repo_override: Some(PathBuf::from("/tmp/workspace")),
            output_json: false,
            fix: false,
            verbose: false,
            explain: None,
        })
    );
}

#[test]
fn parse_command_preserves_command_local_repo_override_over_leading_repo() {
    let cmd = parse_command(vec![
        "--repo".to_owned(),
        "/tmp/global".to_owned(),
        "doctor".to_owned(),
        "--repo".to_owned(),
        "/tmp/local".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Doctor(DoctorArgs {
            repo_override: Some(PathBuf::from("/tmp/local")),
            output_json: false,
            fix: false,
            verbose: false,
            explain: None,
        })
    );
}

#[test]
fn parse_command_applies_leading_repo_and_json_to_tasks_builtin() {
    let cmd = parse_command(vec![
        "--repo".to_owned(),
        "/tmp/workspace".to_owned(),
        "--json".to_owned(),
        "tasks".to_owned(),
        "status".to_owned(),
        "api/dev".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Tasks(TasksArgs {
            repo_override: Some(PathBuf::from("/tmp/workspace")),
            task_name: None,
            resolve_selector: None,
            status_selector: Some("api/dev".to_owned()),
            status_all: false,
            output_json: true,
            pretty_json: true,
        })
    );
}

#[test]
fn parse_command_applies_leading_task_runtime_flags_to_task_selectors() {
    let cmd = parse_command(vec![
        "--repo".to_owned(),
        "/tmp/workspace".to_owned(),
        "--verbose-root".to_owned(),
        "--env-schema".to_owned(),
        "config/env.schema.toml".to_owned(),
        "snapshot".to_owned(),
        "--plan".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Task(TaskInvocation {
            name: "snapshot".to_owned(),
            args: vec![
                "--env-schema".to_owned(),
                "config/env.schema.toml".to_owned(),
                "--verbose-root".to_owned(),
                "--repo".to_owned(),
                "/tmp/workspace".to_owned(),
                "--plan".to_owned(),
            ],
        })
    );
}

#[test]
fn parse_command_rejects_task_only_global_flags_for_builtin_commands() {
    let err = parse_command(vec!["--verbose-root".to_owned(), "doctor".to_owned()])
        .expect_err("parse should fail");
    assert_eq!(err.to_string(), "unknown argument: --verbose-root");
}

#[test]
fn parse_command_allows_leading_repo_on_builtin_help() {
    let cmd = parse_command(vec![
        "--repo".to_owned(),
        "/tmp/workspace".to_owned(),
        "doctor".to_owned(),
        "--help".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Doctor));
}

#[test]
fn parse_tasks_help_is_scoped() {
    let cmd =
        parse_command(vec!["tasks".to_owned(), "--help".to_owned()]).expect("parse should succeed");
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
fn parse_defer_help_is_scoped() {
    let cmd =
        parse_command(vec!["defer".to_owned(), "--help".to_owned()]).expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Defer));
}

#[test]
fn parse_demo_help_is_scoped() {
    let cmd =
        parse_command(vec!["demo".to_owned(), "--help".to_owned()]).expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Demo));
}

#[test]
fn parse_docs_help_is_scoped() {
    let cmd =
        parse_command(vec!["docs".to_owned(), "--help".to_owned()]).expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Docs));
}

#[test]
fn parse_contracts_help_is_scoped() {
    let cmd = parse_command(vec!["contracts".to_owned(), "--help".to_owned()])
        .expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Contracts));
}

#[test]
fn parse_exec_help_is_scoped() {
    let cmd =
        parse_command(vec!["exec".to_owned(), "--help".to_owned()]).expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Exec));
}

#[test]
fn parse_state_help_is_scoped() {
    let cmd =
        parse_command(vec!["state".to_owned(), "--help".to_owned()]).expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::State));
}

#[test]
fn parse_gateway_help_is_scoped() {
    let cmd = parse_command(vec!["gateway".to_owned(), "--help".to_owned()])
        .expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Gateway));
}

#[test]
fn parse_graph_help_is_scoped() {
    let cmd =
        parse_command(vec!["graph".to_owned(), "--help".to_owned()]).expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Graph));
}

#[test]
fn parse_service_help_alias_is_scoped() {
    let cmd = parse_command(vec!["service".to_owned(), "--help".to_owned()])
        .expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Service));
}

#[test]
fn parse_catalog_help_reports_removed_command() {
    let error = parse_command(vec!["catalog".to_owned(), "--help".to_owned()])
        .expect_err("removed catalog command should fail");
    assert!(error.to_string().contains("`effigy catalog` was removed"));
}

#[test]
fn parse_catalogue_help_alias_falls_back_to_task_routing() {
    let catalogue = parse_command(vec!["catalogue".to_owned(), "--help".to_owned()])
        .expect("catalogue token should route as a task selector");

    assert_eq!(
        catalogue,
        Command::Task(TaskInvocation {
            name: "catalogue".to_owned(),
            args: vec!["--help".to_owned()],
        })
    );
}

#[test]
fn parse_container_help_is_scoped() {
    let cmd = parse_command(vec!["container".to_owned(), "--help".to_owned()])
        .expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Container));
}

#[test]
fn parse_system_help_is_scoped() {
    let cmd = parse_command(vec!["system".to_owned(), "--help".to_owned()])
        .expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::System));
}

#[test]
fn parse_workspace_help_is_scoped() {
    let cmd = parse_command(vec!["workspace".to_owned(), "--help".to_owned()])
        .expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Workspace));
}

#[test]
fn parse_help_command_alias_is_general_help() {
    let cmd = parse_command(vec!["help".to_owned()]).expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::General));
}

#[test]
fn parse_tasks_migrate_routes_through_nested_builtin_entrypoint() {
    let cmd = parse_command(vec![
        "tasks".to_owned(),
        "migrate".to_owned(),
        "--apply".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Task(TaskInvocation {
            name: "tasks".to_owned(),
            args: vec!["migrate".to_owned(), "--apply".to_owned()],
        })
    );
}

#[test]
fn parse_tasks_unlock_routes_through_nested_builtin_entrypoint() {
    let cmd = parse_command(vec![
        "tasks".to_owned(),
        "unlock".to_owned(),
        "--all".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Task(TaskInvocation {
            name: "tasks".to_owned(),
            args: vec!["unlock".to_owned(), "--all".to_owned()],
        })
    );
}

#[test]
fn parse_tasks_cache_routes_through_nested_builtin_entrypoint() {
    let cmd = parse_command(vec![
        "tasks".to_owned(),
        "cache".to_owned(),
        "inspect".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Task(TaskInvocation {
            name: "tasks".to_owned(),
            args: vec!["cache".to_owned(), "inspect".to_owned()],
        })
    );
}

#[test]
fn parse_version_flag_is_version_command() {
    let cmd = parse_command(vec!["--version".to_owned()]).expect("parse should succeed");
    assert_eq!(cmd, Command::Version);
}

#[test]
fn parse_version_command_alias_is_version_command() {
    let cmd = parse_command(vec!["version".to_owned()]).expect("parse should succeed");
    assert_eq!(cmd, Command::Version);
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
    assert_eq!(
        cmd,
        Command::Task(TaskInvocation {
            name: "migrate".to_owned(),
            args: vec!["--help".to_owned()],
        })
    );
}
