use crate::tests::prelude::{
    parse_command, Command, DeployArgs, DeploySubcommand, DoctorArgs, PathBuf, TaskInvocation,
    TasksArgs,
};

/// The default `DoctorArgs` shape; rows name the fields they set.
fn base_doctor() -> DoctorArgs {
    DoctorArgs {
        repo_override: None,
        output_json: false,
        fix: false,
        verbose: false,
        deep: false,
        catalog: None,
        all_catalogs: false,
        refresh: false,
        explain: None,
    }
}

/// One row per distinct `doctor` flag mapping in `parse_doctor`: the
/// `--repo`/`--fix`/`--json` combination, `--verbose`, the explanation
/// target with its `--` passthrough and the deep-catalog-refresh triple.
#[test]
fn parse_doctor_flags_map_their_fields() {
    let cases: &[(&[&str], Command)] = &[
        (
            &["doctor", "--repo", "/tmp/repo", "--fix", "--json"],
            Command::Doctor(DoctorArgs {
                repo_override: Some(PathBuf::from("/tmp/repo")),
                output_json: true,
                fix: true,
                ..base_doctor()
            }),
        ),
        (
            &["doctor", "--verbose"],
            Command::Doctor(DoctorArgs {
                verbose: true,
                ..base_doctor()
            }),
        ),
        (
            &["doctor", "catalog_a/build", "--", "--watch"],
            Command::Doctor(DoctorArgs {
                explain: Some(TaskInvocation {
                    name: "catalog_a/build".to_owned(),
                    args: vec!["--".to_owned(), "--watch".to_owned()],
                }),
                ..base_doctor()
            }),
        ),
        (
            &["doctor", "--deep", "--catalog", "api", "--refresh"],
            Command::Doctor(DoctorArgs {
                deep: true,
                catalog: Some("api".to_owned()),
                refresh: true,
                ..base_doctor()
            }),
        ),
    ];
    for (args, expected) in cases {
        assert_eq!(
            &parse_command(args.iter().map(|arg| (*arg).to_owned())).expect("parse should succeed"),
            expected,
            "{args:?}"
        );
    }
}

/// The three `validate_doctor_flags` refusals stay distinct: deep-only
/// flags without `--deep`, `--catalog` with `--all-catalogs`, and deep
/// flags after an explanation selector.
#[test]
fn parse_doctor_refuses_invalid_flag_combinations() {
    let cases: &[(&[&str], &str)] = &[
        (&["doctor", "--all-catalogs"], "require `--deep`"),
        (
            &["doctor", "--deep", "--catalog", "api", "--all-catalogs"],
            "mutually exclusive",
        ),
        (
            &["doctor", "build", "--deep"],
            "explanation mode cannot combine",
        ),
    ];
    for (args, needle) in cases {
        let error =
            parse_command(args.iter().map(|arg| (*arg).to_owned())).expect_err("parse should fail");
        assert!(error.to_string().contains(needle), "{args:?}: {error}");
    }
}

/// One row per distinct `tasks` route: listing filters, bare `--json`,
/// `status` with a selector, the missing-selector refusal route and
/// `status --all`. Full-struct equality pins `qa: None` on every row —
/// qa-group routes never leak into plain status results.
#[test]
fn parse_tasks_routes_map_their_filters() {
    let cases: &[(&[&str], Command)] = &[
        (
            &["tasks", "--repo", "/tmp/repo", "--task", "db:reset"],
            Command::Tasks(TasksArgs {
                all: false,
                repo_override: Some(PathBuf::from("/tmp/repo")),
                task_name: Some("db:reset".to_owned()),
                resolve_selector: None,
                status_selector: None,
                status_all: false,
                output_json: false,
                pretty_json: true,
                qa: None,
            }),
        ),
        (
            &["tasks", "--json"],
            Command::Tasks(TasksArgs {
                all: false,
                repo_override: None,
                task_name: None,
                resolve_selector: None,
                status_selector: None,
                status_all: false,
                output_json: true,
                pretty_json: true,
                qa: None,
            }),
        ),
        (
            &["tasks", "status", "catalog_a/api", "--json"],
            Command::Tasks(TasksArgs {
                all: false,
                repo_override: None,
                task_name: None,
                resolve_selector: None,
                status_selector: Some("catalog_a/api".to_owned()),
                status_all: false,
                output_json: true,
                pretty_json: true,
                qa: None,
            }),
        ),
        (
            &["tasks", "status", "--all", "--json"],
            Command::Tasks(TasksArgs {
                all: false,
                repo_override: None,
                task_name: None,
                resolve_selector: None,
                status_selector: None,
                status_all: true,
                output_json: true,
                pretty_json: true,
                qa: None,
            }),
        ),
    ];
    for (args, expected) in cases {
        assert_eq!(
            &parse_command(args.iter().map(|arg| (*arg).to_owned())).expect("parse should succeed"),
            expected,
            "{args:?}"
        );
    }
    let error = parse_command(vec![
        "tasks".to_owned(),
        "status".to_owned(),
        "--json".to_owned(),
    ])
    .expect_err("parse should fail");
    assert_eq!(error.to_string(), "`tasks status` requires a selector");
}

/// One row per distinct `deploy` subcommand mapping: environment positional
/// and flag spellings per verb, the export provider/path/plan vocabulary,
/// `--write-report`, `apply --yes`, `history --limit` and `redeploy`.
/// `deploy status` is pinned too (it had no parse-level mapping test
/// before; runner-core execution tests cover the action itself).
#[test]
fn parse_deploy_subcommands_map_their_flags() {
    let cases: &[(&[&str], Command)] = &[
        (
            &["deploy", "model", "--repo", "/tmp/repo", "--json"],
            Command::Deploy(DeployArgs {
                subcommand: DeploySubcommand::Model { env: None },
                repo_override: Some(PathBuf::from("/tmp/repo")),
                output_json: true,
            }),
        ),
        (
            &[
                "deploy",
                "export",
                "acme-cloud",
                "--repo",
                "/tmp/repo",
                "--path",
                "infra/deploy",
                "--plan",
                "--json",
            ],
            Command::Deploy(DeployArgs {
                subcommand: DeploySubcommand::Export {
                    provider: "acme-cloud".to_owned(),
                    env: None,
                    path: PathBuf::from("infra/deploy"),
                    plan: true,
                },
                repo_override: Some(PathBuf::from("/tmp/repo")),
                output_json: true,
            }),
        ),
        (
            &["deploy", "model", "production", "--json"],
            Command::Deploy(DeployArgs {
                subcommand: DeploySubcommand::Model {
                    env: Some("production".to_owned()),
                },
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &["deploy", "model", "--env", "uat", "--json"],
            Command::Deploy(DeployArgs {
                subcommand: DeploySubcommand::Model {
                    env: Some("uat".to_owned()),
                },
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &[
                "deploy",
                "export",
                "acme-cloud",
                "production",
                "--path",
                "infra/deploy",
                "--plan",
            ],
            Command::Deploy(DeployArgs {
                subcommand: DeploySubcommand::Export {
                    provider: "acme-cloud".to_owned(),
                    env: Some("production".to_owned()),
                    path: PathBuf::from("infra/deploy"),
                    plan: true,
                },
                repo_override: None,
                output_json: false,
            }),
        ),
        (
            &[
                "deploy",
                "export",
                "acme-cloud",
                "--env",
                "uat",
                "--path",
                "infra/deploy",
            ],
            Command::Deploy(DeployArgs {
                subcommand: DeploySubcommand::Export {
                    provider: "acme-cloud".to_owned(),
                    env: Some("uat".to_owned()),
                    path: PathBuf::from("infra/deploy"),
                    plan: false,
                },
                repo_override: None,
                output_json: false,
            }),
        ),
        (
            &["deploy", "plan", "uat", "--write-report", "--json"],
            Command::Deploy(DeployArgs {
                subcommand: DeploySubcommand::Plan {
                    env: "uat".to_owned(),
                    write_report: true,
                },
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &["deploy", "apply", "production", "--yes", "--json"],
            Command::Deploy(DeployArgs {
                subcommand: DeploySubcommand::Apply {
                    env: "production".to_owned(),
                    yes: true,
                },
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &["deploy", "status", "uat", "--json"],
            Command::Deploy(DeployArgs {
                subcommand: DeploySubcommand::Status {
                    env: "uat".to_owned(),
                },
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &["deploy", "history", "uat", "--limit", "5", "--json"],
            Command::Deploy(DeployArgs {
                subcommand: DeploySubcommand::History {
                    env: "uat".to_owned(),
                    limit: Some(5),
                },
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &[
                "deploy",
                "redeploy",
                "uat",
                "--deployment",
                "deploy-1",
                "--yes",
            ],
            Command::Deploy(DeployArgs {
                subcommand: DeploySubcommand::Redeploy {
                    env: "uat".to_owned(),
                    deployment: "deploy-1".to_owned(),
                    yes: true,
                },
                repo_override: None,
                output_json: false,
            }),
        ),
    ];
    for (args, expected) in cases {
        assert_eq!(
            &parse_command(args.iter().map(|arg| (*arg).to_owned())).expect("parse should succeed"),
            expected,
            "{args:?}"
        );
    }
}

/// Export needs its `--path`; unknown deploy words are refused.
#[test]
fn parse_deploy_refuses_missing_export_path() {
    let error = parse_command(vec![
        "deploy".to_owned(),
        "export".to_owned(),
        "acme-cloud".to_owned(),
    ])
    .expect_err("export without --path must fail");
    assert!(
        error.to_string().contains("--path"),
        "export refusal must name --path: {error}"
    );
    let unknown = parse_command(vec!["deploy".to_owned(), "wat".to_owned()])
        .expect_err("unknown deploy subcommand must fail");
    assert_eq!(unknown.to_string(), "unknown argument: wat");
}
