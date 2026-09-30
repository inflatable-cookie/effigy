use crate::tests::prelude::{
    parse_command, Command, DraftArgs, HelpTopic, PathBuf, TasksQaCommand,
};

#[test]
fn parse_tasks_qa_groups_list_with_filter_and_file() {
    let cmd = parse_command(vec![
        "tasks".to_owned(),
        "qa-groups".to_owned(),
        "list".to_owned(),
        "agent".to_owned(),
        "--file".to_owned(),
        "config/qa-groups/2026-10-02-check.toml".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    let Command::Tasks(args) = cmd else {
        panic!("expected Tasks command");
    };
    assert_eq!(
        args.qa,
        Some(TasksQaCommand::GroupsList {
            filter: Some("agent".to_owned()),
            file: Some(PathBuf::from("config/qa-groups/2026-10-02-check.toml")),
            output_json: true,
            pretty_json: true,
        })
    );
    assert_eq!(
        args.task_name, None,
        "group routes never become task filters"
    );
}

#[test]
fn parse_tasks_qa_groups_requires_the_list_action() {
    let error = parse_command(vec!["tasks".to_owned(), "qa-groups".to_owned()])
        .expect_err("`qa-groups` without `list` must fail");
    assert!(
        error.to_string().contains("supports only `list`"),
        "{error}"
    );
}

#[test]
fn parse_tasks_qa_group_run_with_scopes_and_plan() {
    let cmd = parse_command(vec![
        "tasks".to_owned(),
        "qa-group".to_owned(),
        "run".to_owned(),
        "agent-cli".to_owned(),
        "--scope".to_owned(),
        "cargo-package:effigy-cli".to_owned(),
        "--scope".to_owned(),
        "path:crates/effigy-cli/src/main.rs".to_owned(),
        "--plan".to_owned(),
        "--json".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
    ])
    .expect("parse should succeed");
    let Command::Tasks(args) = cmd else {
        panic!("expected Tasks command");
    };
    assert_eq!(args.repo_override, Some(PathBuf::from("/tmp/repo")));
    assert_eq!(
        args.qa,
        Some(TasksQaCommand::GroupRun {
            selector: "agent-cli".to_owned(),
            file: None,
            scopes: vec![
                "cargo-package:effigy-cli".to_owned(),
                "path:crates/effigy-cli/src/main.rs".to_owned(),
            ],
            plan: true,
            output_json: true,
        })
    );
}

#[test]
fn parse_tasks_qa_group_run_requires_a_selector() {
    let error = parse_command(vec![
        "tasks".to_owned(),
        "qa-group".to_owned(),
        "run".to_owned(),
        "--plan".to_owned(),
    ])
    .expect_err("missing selector must fail");
    assert!(
        error.to_string().contains("requires a group selector"),
        "{error}"
    );
}

#[test]
fn parse_tasks_qa_group_status_and_logs_and_stop() {
    let cmd = parse_command(vec![
        "tasks".to_owned(),
        "qa-group".to_owned(),
        "status".to_owned(),
        "qa-1-2-3".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    let Command::Tasks(args) = cmd else {
        panic!("expected Tasks command");
    };
    assert_eq!(
        args.qa,
        Some(TasksQaCommand::GroupStatus {
            run_id: "qa-1-2-3".to_owned(),
            output_json: true,
        })
    );

    let cmd = parse_command(vec![
        "tasks".to_owned(),
        "qa-group".to_owned(),
        "logs".to_owned(),
        "qa-1-2-3".to_owned(),
        "--follow".to_owned(),
    ])
    .expect("parse should succeed");
    let Command::Tasks(args) = cmd else {
        panic!("expected Tasks command");
    };
    assert_eq!(
        args.qa,
        Some(TasksQaCommand::GroupLogs {
            run_id: "qa-1-2-3".to_owned(),
            follow: true,
        })
    );

    let cmd = parse_command(vec![
        "tasks".to_owned(),
        "qa-group".to_owned(),
        "stop".to_owned(),
        "qa-1-2-3".to_owned(),
    ])
    .expect("stop parses so the runner can reject it precisely");
    let Command::Tasks(args) = cmd else {
        panic!("expected Tasks command");
    };
    assert_eq!(
        args.qa,
        Some(TasksQaCommand::GroupStop {
            run_id: "qa-1-2-3".to_owned(),
            output_json: false,
        })
    );
}

#[test]
fn parse_tasks_qa_group_rejects_unknown_actions() {
    let error = parse_command(vec![
        "tasks".to_owned(),
        "qa-group".to_owned(),
        "prune".to_owned(),
    ])
    .expect_err("unknown action must fail");
    assert!(
        error.to_string().contains("run, status, logs, and stop"),
        "{error}"
    );
}

#[test]
fn parse_bare_qa_group_words_stay_repository_selectors() {
    // The reserved contract surface is the exact multiword form under
    // `tasks`; bare words remain ordinary selectors.
    for bare in ["qa-groups", "qa-group"] {
        let cmd = parse_command(vec![bare.to_owned()]).expect("selector parses");
        assert!(
            matches!(cmd, Command::Task(_)),
            "`{bare}` must stay a repository selector, not a qa-group route"
        );
    }
}

#[test]
fn parse_tasks_status_route_is_unchanged_by_qa_routes() {
    let cmd = parse_command(vec![
        "tasks".to_owned(),
        "status".to_owned(),
        "test".to_owned(),
        "--json".to_owned(),
    ])
    .expect("existing status route unchanged");
    let Command::Tasks(args) = cmd else {
        panic!("expected Tasks command");
    };
    assert_eq!(args.status_selector.as_deref(), Some("test"));
    assert_eq!(args.qa, None);
}

#[test]
fn parse_tasks_qa_group_help_returns_tasks_panel() {
    let cmd = parse_command(vec![
        "tasks".to_owned(),
        "qa-group".to_owned(),
        "run".to_owned(),
        "--help".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Tasks));
}

#[test]
fn parse_draft_accepts_plan_for_non_executing_inspection() {
    // Contract 046 documents `draft <SELECTOR> --plan`; the flag travels in
    // the invocation passthrough so the canonical preflight selects plan
    // mode, exactly like `--json`.
    let cmd = parse_command(vec![
        "draft".to_owned(),
        "heavy-probe".to_owned(),
        "--plan".to_owned(),
    ])
    .expect("draft --plan parses");
    assert_eq!(
        cmd,
        Command::Draft(DraftArgs {
            repo_override: None,
            selector: "heavy-probe".to_owned(),
            args: vec!["--plan".to_owned()],
            output_json: false,
        })
    );
}
