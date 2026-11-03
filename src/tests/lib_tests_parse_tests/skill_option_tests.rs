use crate::tests::prelude::{
    parse_command, Command, PathBuf, SkillArgs, SkillStdioMode, SkillSubcommand, TaskInvocation,
};

/// One row per positive `skill` mapping: the tasks path form and the run
/// forms — passthrough separation, named lookup, typed `--stdio`.
#[test]
fn parse_skill_run_and_tasks_map_paths_selectors_and_stdio() {
    let cases: &[(&[&str], SkillArgs)] = &[
        (
            &["skill", "tasks", "--path", "/opt/skills/example", "--json"],
            SkillArgs {
                subcommand: SkillSubcommand::Tasks {
                    path: PathBuf::from("/opt/skills/example"),
                },
                output_json: true,
            },
        ),
        (
            &[
                "skill",
                "run",
                "--path",
                "/opt/skills/example/effigy.toml",
                "example/check",
                "--repo",
                "/work/consumer",
                "--json",
                "--",
                "--repo",
                "literal-task-arg",
            ],
            SkillArgs {
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
            },
        ),
        (
            &["skill", "run", "northstar/retired-concepts"],
            SkillArgs {
                subcommand: SkillSubcommand::Run {
                    path: None,
                    stdio: SkillStdioMode::Default,
                    task: TaskInvocation {
                        name: "northstar/retired-concepts".to_owned(),
                        args: Vec::new(),
                    },
                    repo_override: None,
                },
                output_json: false,
            },
        ),
        (
            &[
                "skill",
                "run",
                "northstar/retired-concepts",
                "--stdio",
                "passthrough",
            ],
            SkillArgs {
                subcommand: SkillSubcommand::Run {
                    path: None,
                    stdio: SkillStdioMode::Passthrough,
                    task: TaskInvocation {
                        name: "northstar/retired-concepts".to_owned(),
                        args: Vec::new(),
                    },
                    repo_override: None,
                },
                output_json: false,
            },
        ),
    ];
    for (args, expected) in cases {
        assert_eq!(
            parse_command(args.iter().map(|arg| (*arg).to_owned())).expect("parse should succeed"),
            Command::Skill(expected.clone()),
            "{args:?}"
        );
    }
}

/// Skill refusals and the json×stdio incompatibility. The local `--json`
/// conflict is detected in `parse_skill_command`; the leading-global one in
/// `apply_global_cli_options` — two distinct branches, both kept.
#[test]
fn parse_skill_refuses_unqualified_selectors_and_missing_arguments() {
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

    let invalid_stdio = parse_command(vec![
        "skill".to_owned(),
        "run".to_owned(),
        "northstar/retired-concepts".to_owned(),
        "--stdio".to_owned(),
        "raw".to_owned(),
    ])
    .expect_err("--stdio only accepts passthrough");
    assert!(
        invalid_stdio.to_string().contains("passthrough"),
        "{invalid_stdio}"
    );

    let local = parse_command(vec![
        "skill".to_owned(),
        "run".to_owned(),
        "northstar/retired-concepts".to_owned(),
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
        "northstar/retired-concepts".to_owned(),
        "--stdio".to_owned(),
        "passthrough".to_owned(),
    ])
    .expect_err("global --json conflicts with passthrough");
    assert!(global.to_string().contains("incompatible"), "{global}");
}
