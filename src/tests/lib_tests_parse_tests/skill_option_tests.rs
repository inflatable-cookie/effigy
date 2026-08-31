use crate::tests::prelude::{
    parse_command, Command, HelpTopic, PathBuf, SkillArgs, SkillSubcommand, TaskInvocation,
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
                path: PathBuf::from("/opt/skills/example/effigy.toml"),
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
