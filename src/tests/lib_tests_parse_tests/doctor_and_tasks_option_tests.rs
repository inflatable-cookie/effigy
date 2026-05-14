use crate::tests::prelude::{
    parse_command, Command, DeployArgs, DeploySubcommand, DoctorArgs, PathBuf, TaskInvocation,
    TasksArgs,
};

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
        "catalog_a/build".to_owned(),
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
                name: "catalog_a/build".to_owned(),
                args: vec!["--".to_owned(), "--watch".to_owned()],
            }),
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
        "db:reset".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Tasks(TasksArgs {
            repo_override: Some(PathBuf::from("/tmp/repo")),
            task_name: Some("db:reset".to_owned()),
            resolve_selector: None,
            status_selector: None,
            status_all: false,
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
            status_selector: None,
            status_all: false,
            output_json: true,
            pretty_json: true,
        })
    );
}

#[test]
fn parse_tasks_status_with_selector_and_json() {
    let cmd = parse_command(vec![
        "tasks".to_owned(),
        "status".to_owned(),
        "catalog_a/api".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Tasks(TasksArgs {
            repo_override: None,
            task_name: None,
            resolve_selector: None,
            status_selector: Some("catalog_a/api".to_owned()),
            status_all: false,
            output_json: true,
            pretty_json: true,
        })
    );
}

#[test]
fn parse_tasks_status_requires_selector() {
    let error = parse_command(vec![
        "tasks".to_owned(),
        "status".to_owned(),
        "--json".to_owned(),
    ])
    .expect_err("parse should fail");
    assert_eq!(error.to_string(), "`tasks status` requires a selector");
}

#[test]
fn parse_tasks_status_all_with_json() {
    let cmd = parse_command(vec![
        "tasks".to_owned(),
        "status".to_owned(),
        "--all".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Tasks(TasksArgs {
            repo_override: None,
            task_name: None,
            resolve_selector: None,
            status_selector: None,
            status_all: true,
            output_json: true,
            pretty_json: true,
        })
    );
}

#[test]
fn parse_deploy_model_with_repo_and_json() {
    let cmd = parse_command(vec![
        "deploy".to_owned(),
        "model".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Deploy(DeployArgs {
            subcommand: DeploySubcommand::Model,
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_deploy_export_provider_with_path_plan_and_json() {
    let cmd = parse_command(vec![
        "deploy".to_owned(),
        "export".to_owned(),
        "acme-cloud".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--path".to_owned(),
        "infra/deploy".to_owned(),
        "--plan".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Deploy(DeployArgs {
            subcommand: DeploySubcommand::Export {
                provider: "acme-cloud".to_owned(),
                path: PathBuf::from("infra/deploy"),
                plan: true,
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_deploy_plan_with_env_write_report_and_json() {
    let cmd = parse_command(vec![
        "deploy".to_owned(),
        "plan".to_owned(),
        "uat".to_owned(),
        "--write-report".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Deploy(DeployArgs {
            subcommand: DeploySubcommand::Plan {
                env: "uat".to_owned(),
                write_report: true,
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_deploy_apply_requires_yes_in_model_not_parser() {
    let cmd = parse_command(vec![
        "deploy".to_owned(),
        "apply".to_owned(),
        "production".to_owned(),
        "--yes".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Deploy(DeployArgs {
            subcommand: DeploySubcommand::Apply {
                env: "production".to_owned(),
                yes: true,
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_deploy_history_with_limit() {
    let cmd = parse_command(vec![
        "deploy".to_owned(),
        "history".to_owned(),
        "uat".to_owned(),
        "--limit".to_owned(),
        "5".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Deploy(DeployArgs {
            subcommand: DeploySubcommand::History {
                env: "uat".to_owned(),
                limit: Some(5),
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_deploy_redeploy_with_deployment_and_yes() {
    let cmd = parse_command(vec![
        "deploy".to_owned(),
        "redeploy".to_owned(),
        "uat".to_owned(),
        "--deployment".to_owned(),
        "deploy-1".to_owned(),
        "--yes".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Deploy(DeployArgs {
            subcommand: DeploySubcommand::Redeploy {
                env: "uat".to_owned(),
                deployment: "deploy-1".to_owned(),
                yes: true,
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_deploy_export_provider_after_global_json_and_repo() {
    let cmd = parse_command(vec![
        "deploy".to_owned(),
        "export".to_owned(),
        "acme-cloud".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--path".to_owned(),
        "infra/deploy".to_owned(),
        "--plan".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Deploy(DeployArgs {
            subcommand: DeploySubcommand::Export {
                provider: "acme-cloud".to_owned(),
                path: PathBuf::from("infra/deploy"),
                plan: true,
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}
