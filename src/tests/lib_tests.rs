use super::{
    apply_global_json_flag, command_requests_json, parse_command, render_cli_header, render_help,
    strip_global_json_flag, strip_global_json_flags, Command, DoctorArgs, HelpTopic,
    TaskInvocation, TasksArgs,
};
use crate::ui::PlainRenderer;
use std::path::PathBuf;

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
fn render_help_writes_structured_sections() {
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false);
    render_help(&mut renderer, HelpTopic::General).expect("help render");
    let rendered = String::from_utf8(renderer.into_inner()).expect("utf8");
    assert!(rendered.contains("Commands"));
    assert!(rendered.contains("effigy help"));
    assert!(rendered.contains("effigy config"));
    assert!(rendered.contains("effigy doctor"));
    assert!(rendered.contains("effigy test"));
    assert!(rendered.contains("<catalog>/test fallback"));
    assert!(!rendered.contains("effigy test --plan"));
    assert!(rendered.contains("Use `effigy <built-in-task> --help`"));
    assert!(!rendered.contains("Quick Start"));
    assert!(!rendered.contains("effigy Help"));
}

#[test]
fn render_doctor_help_shows_fix_and_json_options() {
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false);
    render_help(&mut renderer, HelpTopic::Doctor).expect("help render");
    let rendered = String::from_utf8(renderer.into_inner()).expect("utf8");
    assert!(rendered.contains("doctor Help"));
    assert!(rendered.contains("--fix"));
    assert!(rendered.contains("--verbose"));
    assert!(rendered.contains("--json"));
    assert!(rendered.contains("effigy doctor --fix"));
    assert!(rendered.contains("effigy doctor --verbose"));
    assert!(rendered.contains("effigy doctor <task> <args>"));
    assert!(rendered.contains("effigy doctor farmyard/build -- --watch"));
}

#[test]
fn render_tasks_help_shows_resolve_and_json_options() {
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false);
    render_help(&mut renderer, HelpTopic::Tasks).expect("help render");
    let rendered = String::from_utf8(renderer.into_inner()).expect("utf8");
    assert!(rendered.contains("tasks Help"));
    assert!(rendered.contains("--resolve <SELECTOR>"));
    assert!(rendered.contains("routing probes only when debugging selector resolution"));
    assert!(rendered.contains("--json"));
    assert!(rendered.contains("--pretty <true|false>"));
    assert!(rendered.contains("effigy tasks --resolve <catalog>/<task>"));
    assert!(rendered.contains("effigy tasks --json --resolve test"));
}

#[test]
fn render_test_help_shows_detection_and_config() {
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false);
    render_help(&mut renderer, HelpTopic::Test).expect("help render");
    let rendered = String::from_utf8(renderer.into_inner()).expect("utf8");
    assert!(rendered.contains("test Help"));
    assert!(rendered.contains("built-in test runner detection by default"));
    assert!(rendered.contains("`tasks.test` is defined, it takes precedence"));
    assert!(rendered.contains("<catalog>/test fallback"));
    assert!(rendered.contains("Detection Order"));
    assert!(rendered.contains("--verbose-results"));
    assert!(rendered.contains("--tui"));
    assert!(rendered.contains("[suite] [runner args]"));
    assert!(rendered.contains("effigy test vitest user-service"));
    assert!(rendered.contains("effigy <catalog>/test"));
    assert!(rendered.contains("effigy test --plan user-service"));
    assert!(rendered.contains("effigy test --plan viteest user-service"));
    assert!(rendered.contains("Named Test Selection"));
    assert!(rendered.contains("effigy test user-service"));
    assert!(rendered.contains("prefix the suite explicitly"));
    assert!(rendered.contains("check `available-suites` per target"));
    assert!(rendered.contains("suggests nearest suite names"));
    assert!(rendered.contains("source of truth and auto-detection is skipped"));
    assert!(rendered.contains("Migration"));
    assert!(rendered.contains("ambiguous in multi-suite repos"));
    assert!(rendered.contains("effigy test viteest user-service"));
    assert!(rendered.contains("suggests `effigy test vitest user-service`"));
    assert!(rendered.contains("effigy test nextest user_service --nocapture"));
    assert!(rendered.contains("Error Recovery"));
    assert!(rendered.contains("Ambiguity: `effigy test user-service`"));
    assert!(rendered.contains("Unavailable or mistyped suite"));
    assert!(rendered.contains("[package_manager]"));
    assert!(rendered.contains("js = \"bun\""));
    assert!(rendered.contains("[test]"));
    assert!(rendered.contains("max_parallel = 2"));
    assert!(rendered.contains("[test.suites]"));
    assert!(rendered.contains("unit = \"bun x vitest run\""));
    assert!(rendered.contains("[test.runners]"));
    assert!(rendered.contains("vitest = \"bun x vitest run\""));
    assert!(!rendered.contains("[tasks.test]"));
    assert!(rendered.contains("Task-ref chain with quoted args"));
    assert!(rendered.contains(
        "run = [{ task = \"test vitest \\\"user service\\\"\" }, \"printf validate-ok\"]"
    ));
    assert!(rendered.contains("Task-ref chain parsing is shell-like tokenization only"));
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
