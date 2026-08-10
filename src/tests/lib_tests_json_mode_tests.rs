use super::prelude::{
    apply_global_json_flag, command_requests_json, parse_command, BootstrapArgs,
    BootstrapSubcommand, Command, DemoArgs, DemoListQuery, DemoSubcommand, DeployArgs,
    DeploySubcommand, DoctorArgs, GatewayArgs, GatewaySubcommand, ReleaseArgs, ReleaseSubcommand,
    TaskInvocation, TasksArgs,
};

#[test]
fn apply_global_json_flag_injects_task_arg_when_missing() {
    let cmd = Command::Task(TaskInvocation {
        name: "tasks".to_owned(),
        args: vec!["--resolve".to_owned(), "catalog-a/api".to_owned()],
    });
    let applied = apply_global_json_flag(cmd, true);
    match applied {
        Command::Task(task) => {
            assert_eq!(task.args.first(), Some(&"--json".to_owned()));
        }
        other => panic!("expected task command, got: {other:?}"),
    }
}

#[test]
fn apply_global_json_flag_preserves_config_completion_nested_subcommand_position() {
    let cmd = Command::Task(TaskInvocation {
        name: "config".to_owned(),
        args: vec![
            "completion".to_owned(),
            "bash".to_owned(),
            "--export".to_owned(),
        ],
    });
    let applied = apply_global_json_flag(cmd, true);
    match applied {
        Command::Task(task) => {
            assert_eq!(
                task.args,
                vec![
                    "completion".to_owned(),
                    "--json".to_owned(),
                    "bash".to_owned(),
                    "--export".to_owned(),
                ]
            );
        }
        other => panic!("expected task command, got: {other:?}"),
    }
}

#[test]
fn command_requests_json_checks_task_or_global_mode() {
    let version_cmd = Command::Version;
    let cmd = Command::Task(TaskInvocation {
        name: "tasks".to_owned(),
        args: vec!["--resolve".to_owned(), "catalog-a/api".to_owned()],
    });
    assert!(!command_requests_json(&version_cmd, false));
    assert!(command_requests_json(&version_cmd, true));
    assert!(!command_requests_json(&cmd, false));
    assert!(command_requests_json(&cmd, true));

    let cmd_with_json = Command::Task(TaskInvocation {
        name: "tasks".to_owned(),
        args: vec!["--json".to_owned()],
    });
    assert!(command_requests_json(&cmd_with_json, false));

    let cmd_tasks = Command::Tasks(TasksArgs {
        repo_override: None,
        task_name: None,
        resolve_selector: None,
        status_selector: None,
        status_all: false,
        output_json: true,
        pretty_json: true,
    });
    assert!(command_requests_json(&cmd_tasks, false));

    let cmd_doctor = Command::Doctor(DoctorArgs {
        repo_override: None,
        output_json: true,
        fix: false,
        verbose: false,
        explain: None,
    });
    let cmd_gateway = Command::Gateway(GatewayArgs {
        subcommand: GatewaySubcommand::Status,
        output_json: true,
    });
    let cmd_demo = Command::Demo(DemoArgs {
        subcommand: DemoSubcommand::List {
            query: DemoListQuery::default(),
        },
        repo_override: None,
        output_json: true,
    });
    let cmd_bootstrap = Command::Bootstrap(BootstrapArgs {
        subcommand: BootstrapSubcommand::Clone {
            repo_url: "git@github.com:inflatable-cookie/effigy.git".to_owned(),
            path: None,
            branch: None,
            backend: None,
            db_seeds: Vec::new(),
            fresh: false,
            no_prompt: false,
            reuse_path: false,
            start: true,
            plan: true,
        },
        output_json: true,
    });
    let cmd_release = Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Status { check_gates: false },
        repo_override: None,
        output_json: true,
    });
    let cmd_deploy = Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Model,
        repo_override: None,
        output_json: true,
    });
    assert!(command_requests_json(&cmd_doctor, false));
    assert!(command_requests_json(&cmd_gateway, false));
    assert!(command_requests_json(&cmd_demo, false));
    assert!(command_requests_json(&cmd_bootstrap, false));
    assert!(command_requests_json(&cmd_release, false));
    assert!(command_requests_json(&cmd_deploy, false));
}

#[test]
fn apply_global_json_flag_sets_non_task_command_json_mode() {
    let version_cmd = Command::Version;
    let tasks_cmd = Command::Tasks(TasksArgs {
        repo_override: None,
        task_name: None,
        resolve_selector: None,
        status_selector: None,
        status_all: false,
        output_json: false,
        pretty_json: true,
    });
    let doctor_cmd = Command::Doctor(DoctorArgs {
        repo_override: None,
        output_json: false,
        fix: false,
        verbose: false,
        explain: None,
    });
    let gateway_cmd = Command::Gateway(GatewayArgs {
        subcommand: GatewaySubcommand::Status,
        output_json: false,
    });
    let demo_cmd = Command::Demo(DemoArgs {
        subcommand: DemoSubcommand::List {
            query: DemoListQuery::default(),
        },
        repo_override: None,
        output_json: false,
    });
    let bootstrap_cmd = Command::Bootstrap(BootstrapArgs {
        subcommand: BootstrapSubcommand::Clone {
            repo_url: "git@github.com:inflatable-cookie/effigy.git".to_owned(),
            path: None,
            branch: None,
            backend: None,
            db_seeds: Vec::new(),
            fresh: false,
            no_prompt: false,
            reuse_path: false,
            start: true,
            plan: true,
        },
        output_json: false,
    });
    let release_cmd = Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Status { check_gates: false },
        repo_override: None,
        output_json: false,
    });
    let deploy_cmd = Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Model,
        repo_override: None,
        output_json: false,
    });

    let version_applied = apply_global_json_flag(version_cmd, true);
    let tasks_applied = apply_global_json_flag(tasks_cmd, true);
    let doctor_applied = apply_global_json_flag(doctor_cmd, true);
    let gateway_applied = apply_global_json_flag(gateway_cmd, true);
    let demo_applied = apply_global_json_flag(demo_cmd, true);
    let bootstrap_applied = apply_global_json_flag(bootstrap_cmd, true);
    let release_applied = apply_global_json_flag(release_cmd, true);
    let deploy_applied = apply_global_json_flag(deploy_cmd, true);
    assert_eq!(version_applied, Command::Version);
    match tasks_applied {
        Command::Tasks(args) => assert!(args.output_json),
        other => panic!("expected tasks command, got: {other:?}"),
    }
    match doctor_applied {
        Command::Doctor(args) => assert!(args.output_json),
        other => panic!("expected doctor command, got: {other:?}"),
    }
    match gateway_applied {
        Command::Gateway(args) => assert!(args.output_json),
        other => panic!("expected gateway command, got: {other:?}"),
    }
    match demo_applied {
        Command::Demo(args) => assert!(args.output_json),
        other => panic!("expected demo command, got: {other:?}"),
    }
    match bootstrap_applied {
        Command::Bootstrap(args) => assert!(args.output_json),
        other => panic!("expected bootstrap command, got: {other:?}"),
    }
    match release_applied {
        Command::Release(args) => assert!(args.output_json),
        other => panic!("expected release command, got: {other:?}"),
    }
    match deploy_applied {
        Command::Deploy(args) => assert!(args.output_json),
        other => panic!("expected deploy command, got: {other:?}"),
    }
}

#[test]
fn parse_command_applies_leading_json_to_builtin_commands() {
    let command = parse_command(vec!["--json".to_owned(), "doctor".to_owned()])
        .expect("parse should succeed");

    assert!(matches!(
        command,
        Command::Doctor(DoctorArgs {
            output_json: true,
            ..
        })
    ));
}
