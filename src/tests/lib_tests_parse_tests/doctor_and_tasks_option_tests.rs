use crate::tests::prelude::{
    parse_command, Command, DeployArgs, DeploySubcommand, DoctorArgs, PathBuf, TaskInvocation,
    TasksArgs,
};
use effigy_cli::DeployExportProvider;

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
fn parse_deploy_export_render_with_path_plan_and_json() {
    let cmd = parse_command(vec![
        "deploy".to_owned(),
        "export".to_owned(),
        "render".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--path".to_owned(),
        "infra/render".to_owned(),
        "--plan".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Deploy(DeployArgs {
            subcommand: DeploySubcommand::Export {
                provider: DeployExportProvider::Render,
                path: PathBuf::from("infra/render"),
                plan: true,
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}
