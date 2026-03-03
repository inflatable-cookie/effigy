use super::prelude::*;

#[test]
fn apply_global_json_flag_injects_task_arg_when_missing() {
    let cmd = Command::Task(TaskInvocation {
        name: "catalogs".to_owned(),
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
fn command_requests_json_checks_task_or_global_mode() {
    let cmd = Command::Task(TaskInvocation {
        name: "catalogs".to_owned(),
        args: vec!["--resolve".to_owned(), "catalog-a/api".to_owned()],
    });
    assert!(!command_requests_json(&cmd, false));
    assert!(command_requests_json(&cmd, true));

    let cmd_with_json = Command::Task(TaskInvocation {
        name: "catalogs".to_owned(),
        args: vec!["--json".to_owned()],
    });
    assert!(command_requests_json(&cmd_with_json, false));

    let cmd_tasks = Command::Tasks(TasksArgs {
        repo_override: None,
        task_name: None,
        resolve_selector: None,
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
    assert!(command_requests_json(&cmd_doctor, false));
}

#[test]
fn apply_global_json_flag_sets_non_task_command_json_mode() {
    let tasks_cmd = Command::Tasks(TasksArgs {
        repo_override: None,
        task_name: None,
        resolve_selector: None,
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

    let tasks_applied = apply_global_json_flag(tasks_cmd, true);
    let doctor_applied = apply_global_json_flag(doctor_cmd, true);
    match tasks_applied {
        Command::Tasks(args) => assert!(args.output_json),
        other => panic!("expected tasks command, got: {other:?}"),
    }
    match doctor_applied {
        Command::Doctor(args) => assert!(args.output_json),
        other => panic!("expected doctor command, got: {other:?}"),
    }
}
