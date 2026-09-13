use crate::tests::prelude::{
    parse_command, Command, HelpTopic, PathBuf, SkillArgs, SkillStdioMode, SkillSubcommand,
    TaskInvocation,
};

#[test]
fn parse_skill_tasks_requires_explicit_path() {
    let command = parse_command(vec![
        "skill".to_owned(),
        "tasks".to_owned(),
        "--path".to_owned(),
        "/opt/skills/example".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse skill tasks");
    assert_eq!(
        command,
        Command::Skill(SkillArgs {
            subcommand: SkillSubcommand::Tasks {
                path: PathBuf::from("/opt/skills/example"),
            },
            output_json: true,
        })
    );
}

#[test]
fn parse_skill_run_keeps_source_target_and_passthrough_separate() {
    let command = parse_command(vec![
        "skill".to_owned(),
        "run".to_owned(),
        "--path".to_owned(),
        "/opt/skills/example/effigy.toml".to_owned(),
        "example/check".to_owned(),
        "--repo".to_owned(),
        "/work/consumer".to_owned(),
        "--json".to_owned(),
        "--".to_owned(),
        "--repo".to_owned(),
        "literal-task-arg".to_owned(),
    ])
    .expect("parse skill run");
    assert_eq!(
        command,
        Command::Skill(SkillArgs {
            subcommand: SkillSubcommand::Run {
                path: Some(PathBuf::from("/opt/skills/example/effigy.toml")),
                stdio: SkillStdioMode::Default,
                task: TaskInvocation {
                    name: "example/check".to_owned(),
                    args: vec![
                        "--".to_owned(),
                        "--repo".to_owned(),
                        "literal-task-arg".to_owned(),
                    ],
                },
                repo_override: Some(PathBuf::from("/work/consumer")),
            },
            output_json: true,
        })
    );
}

#[test]
fn parse_skill_run_allows_named_lookup_without_path() {
    let command = parse_command(vec![
        "skill".to_owned(),
        "run".to_owned(),
        "northstar/queue:hook".to_owned(),
    ])
    .expect("parse named skill run");
    assert_eq!(
        command,
        Command::Skill(SkillArgs {
            subcommand: SkillSubcommand::Run {
                path: None,
                stdio: SkillStdioMode::Default,
                task: TaskInvocation {
                    name: "northstar/queue:hook".to_owned(),
                    args: Vec::new(),
                },
                repo_override: None,
            },
            output_json: false,
        })
    );
}

#[test]
fn parse_skill_run_requires_a_qualified_selector_without_path() {
    let unqualified = parse_command(vec![
        "skill".to_owned(),
        "run".to_owned(),
        "identity".to_owned(),
    ])
    .expect_err("named lookup needs a qualified selector");
    assert!(
        unqualified.to_string().contains("qualified"),
        "{unqualified}"
    );

    for selector in ["northstar/", "/hook", "../escape/hook"] {
        let error = parse_command(vec![
            "skill".to_owned(),
            "run".to_owned(),
            selector.to_owned(),
        ])
        .expect_err("invalid skill segment");
        assert!(
            error.to_string().contains("qualified") || error.to_string().contains("invalid skill"),
            "{selector}: {error}"
        );
    }
}

#[test]
fn parse_skill_stdio_passthrough_is_typed() {
    let command = parse_command(vec![
        "skill".to_owned(),
        "run".to_owned(),
        "northstar/queue:hook".to_owned(),
        "--stdio".to_owned(),
        "passthrough".to_owned(),
    ])
    .expect("parse passthrough");
    let Command::Skill(args) = command else {
        panic!("expected skill command");
    };
    assert!(args.stdio_passthrough());
    assert!(!args.output_json);

    let invalid = parse_command(vec![
        "skill".to_owned(),
        "run".to_owned(),
        "northstar/queue:hook".to_owned(),
        "--stdio".to_owned(),
        "raw".to_owned(),
    ])
    .expect_err("--stdio only accepts passthrough");
    assert!(invalid.to_string().contains("passthrough"), "{invalid}");
}

#[test]
fn parse_skill_run_rejects_json_with_stdio_passthrough_in_both_flag_positions() {
    let local = parse_command(vec![
        "skill".to_owned(),
        "run".to_owned(),
        "northstar/queue:hook".to_owned(),
        "--stdio".to_owned(),
        "passthrough".to_owned(),
        "--json".to_owned(),
    ])
    .expect_err("local --json conflicts with passthrough");
    assert!(local.to_string().contains("incompatible"), "{local}");

    let global = parse_command(vec![
        "--json".to_owned(),
        "skill".to_owned(),
        "run".to_owned(),
        "northstar/queue:hook".to_owned(),
        "--stdio".to_owned(),
        "passthrough".to_owned(),
    ])
    .expect_err("global --json conflicts with passthrough");
    assert!(global.to_string().contains("incompatible"), "{global}");
}

#[test]
fn parse_skill_help_is_scoped() {
    assert_eq!(
        parse_command(vec!["skill".to_owned(), "--help".to_owned()]).expect("skill help"),
        Command::Help(HelpTopic::Skill)
    );
}

#[test]
fn parse_skill_rejects_missing_path_and_selector() {
    let missing_path =
        parse_command(vec!["skill".to_owned(), "tasks".to_owned()]).expect_err("path is required");
    assert!(missing_path.to_string().contains("requires --path"));
    let missing_selector = parse_command(vec![
        "skill".to_owned(),
        "run".to_owned(),
        "--path".to_owned(),
        "/opt/skills/example".to_owned(),
    ])
    .expect_err("selector is required");
    assert!(missing_selector
        .to_string()
        .contains("requires a task selector"));
}
