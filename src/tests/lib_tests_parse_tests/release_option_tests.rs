use crate::tests::prelude::{parse_command, Command, PathBuf, ReleaseArgs, ReleaseSubcommand};

/// One row per read/plan `release` mapping: status with its gate check,
/// gates, `run --to`, `--plan`, the `--version` override and the `--dry-run`
/// alias of plan.
#[test]
fn parse_release_read_and_plan_commands_map_their_flags() {
    let cases: &[(&[&str], Command)] = &[
        (
            &[
                "release",
                "status",
                "--repo",
                "/tmp/repo",
                "--check-gates",
                "--json",
            ],
            Command::Release(ReleaseArgs {
                subcommand: ReleaseSubcommand::Status { check_gates: true },
                repo_override: Some(PathBuf::from("/tmp/repo")),
                output_json: true,
            }),
        ),
        (
            &["release", "gates", "--repo", "/tmp/repo", "--json"],
            Command::Release(ReleaseArgs {
                subcommand: ReleaseSubcommand::Gates,
                repo_override: Some(PathBuf::from("/tmp/repo")),
                output_json: true,
            }),
        ),
        (
            &[
                "release",
                "run",
                "--repo",
                "/tmp/repo",
                "--to",
                "certify",
                "--json",
            ],
            Command::Release(ReleaseArgs {
                subcommand: ReleaseSubcommand::Run {
                    to: Some("certify".to_owned()),
                },
                repo_override: Some(PathBuf::from("/tmp/repo")),
                output_json: true,
            }),
        ),
        (
            &["release", "--plan", "--repo", "/tmp/repo", "--json"],
            Command::Release(ReleaseArgs {
                subcommand: ReleaseSubcommand::Plan {
                    version_override: None,
                },
                repo_override: Some(PathBuf::from("/tmp/repo")),
                output_json: true,
            }),
        ),
        (
            &["release", "--plan", "--version", "0.2.8", "--json"],
            Command::Release(ReleaseArgs {
                subcommand: ReleaseSubcommand::Plan {
                    version_override: Some("0.2.8".to_owned()),
                },
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &["release", "--dry-run", "--json"],
            Command::Release(ReleaseArgs {
                subcommand: ReleaseSubcommand::Plan {
                    version_override: None,
                },
                repo_override: None,
                output_json: true,
            }),
        ),
    ];
    for (args, expected) in cases {
        assert_eq!(
            parse_command(args.iter().map(|arg| (*arg).to_owned())).expect("parse should succeed"),
            expected.clone(),
            "{args:?}"
        );
    }
}

/// The run-id verbs (resume, approve, decide, receipt, settle), their
/// refusals, and the `decision` alias of `decide`.
#[test]
fn parse_release_run_id_verbs_and_refusals() {
    let cases: &[(&[&str], ReleaseSubcommand, bool)] = &[
        (
            &["release", "resume", "run-abc"],
            ReleaseSubcommand::ResumeRun {
                run: "run-abc".to_owned(),
            },
            false,
        ),
        (
            &["release", "approve", "run-abc"],
            ReleaseSubcommand::Approve {
                run: "run-abc".to_owned(),
            },
            false,
        ),
        (
            &["release", "decide", "run-abc", "--published"],
            ReleaseSubcommand::Decide {
                run: "run-abc".to_owned(),
                published: true,
            },
            false,
        ),
        (
            &["release", "decide", "run-abc", "--not-published"],
            ReleaseSubcommand::Decide {
                run: "run-abc".to_owned(),
                published: false,
            },
            false,
        ),
        (
            &["release", "decision", "run-abc", "--published"],
            ReleaseSubcommand::Decide {
                run: "run-abc".to_owned(),
                published: true,
            },
            false,
        ),
        (
            &["release", "receipt"],
            ReleaseSubcommand::Receipt { run: None },
            false,
        ),
        (
            &["release", "settle", "run-id", "--json"],
            ReleaseSubcommand::Settle {
                run: "run-id".to_owned(),
            },
            true,
        ),
    ];
    for (args, subcommand, output_json) in cases {
        assert_eq!(
            parse_command(args.iter().map(|arg| (*arg).to_owned())).expect("parse should succeed"),
            Command::Release(ReleaseArgs {
                subcommand: subcommand.clone(),
                repo_override: None,
                output_json: *output_json,
            }),
            "{args:?}"
        );
    }

    // Refusals: a resume needs the Run id; the prepared-state resume is
    // retired; a decision needs its published flag; settle takes exactly
    // one run.
    let missing = parse_command(vec!["release".to_owned(), "resume".to_owned()])
        .expect_err("a resume needs the Run id");
    assert!(missing.to_string().contains("release Run id"), "{missing}");

    let stale = parse_command(vec![
        "release".to_owned(),
        "resume".to_owned(),
        "--allow-stale".to_owned(),
    ])
    .expect_err("the prepared-state resume is retired");
    assert!(stale.to_string().contains("--allow-stale"), "{stale}");

    let unclear = parse_command(vec![
        "release".to_owned(),
        "decide".to_owned(),
        "run-abc".to_owned(),
    ])
    .expect_err("a decision needs --published or --not-published");
    assert!(unclear.to_string().contains("--published"), "{unclear}");

    assert!(parse_command(vec!["release".into(), "settle".into()]).is_err());
    assert!(parse_command(vec![
        "release".into(),
        "settle".into(),
        "one".into(),
        "two".into()
    ])
    .is_err());
}

#[test]
fn parse_release_effigy_only_commands_name_their_repo_task() {
    for (retired, replacement) in [
        ("validate", "release:metadata"),
        ("preflight", "release:preflight"),
        ("proof", "release:proof"),
        ("check-binary", "release:check-binary"),
        ("verify-install", "release:verify:install"),
        ("evidence", "release:receipt_or_evidence"),
    ] {
        let error = parse_command(vec!["release".to_owned(), retired.to_owned()])
            .expect_err("the retired subcommand refuses");
        let message = error.to_string();
        assert!(
            message.contains("retired in v0.15"),
            "{}: {message}",
            retired
        );
        let expected = if replacement == "release:receipt_or_evidence" {
            "release receipt"
        } else {
            replacement
        };
        assert!(message.contains(expected), "{}: {message}", retired);
    }
}

#[test]
fn parse_release_retired_preflight_names_the_pre_gate_stage() {
    let error = parse_command(vec!["release".to_owned(), "preflight".to_owned()])
        .expect_err("the retired subcommand refuses");
    let message = error.to_string();
    // Contract 054 makes preflight a pre-prepare gate, so the refusal names
    // `pre-gates` and the repo task that replaced the command, never `certify`.
    assert!(message.contains("`pre-gates` stage"), "{message}");
    assert!(message.contains("effigy release:preflight"), "{message}");
    assert!(!message.contains("`certify` gate"), "{message}");
}

#[test]
fn parse_release_plan_replaces_the_retired_simulate_command() {
    let error = parse_command(vec!["release".to_owned(), "simulate".to_owned()])
        .expect_err("release simulate must be refused");
    let message = format!("{error:?}");
    assert!(message.contains("retired"), "{message}");
    assert!(message.contains("effigy release --plan"), "{message}");
}
