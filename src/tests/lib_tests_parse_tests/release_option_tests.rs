use crate::tests::prelude::{parse_command, Command, PathBuf, ReleaseArgs, ReleaseSubcommand};

#[test]
fn parse_release_status_with_repo_and_gate_check() {
    let cmd = parse_command(vec![
        "release".to_owned(),
        "status".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--check-gates".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Release(ReleaseArgs {
            subcommand: ReleaseSubcommand::Status { check_gates: true },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_release_gates_with_repo() {
    let cmd = parse_command(vec![
        "release".to_owned(),
        "gates".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Release(ReleaseArgs {
            subcommand: ReleaseSubcommand::Gates,
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_release_run_with_repo_to_and_json() {
    let cmd = parse_command(vec![
        "release".to_owned(),
        "run".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--to".to_owned(),
        "certify".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Release(ReleaseArgs {
            subcommand: ReleaseSubcommand::Run {
                to: Some("certify".to_owned())
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_release_resume_requires_a_run_id() {
    let error = parse_command(vec!["release".to_owned(), "resume".to_owned()])
        .expect_err("a resume needs the Run id");
    assert!(error.to_string().contains("release Run id"), "{error}");

    let error = parse_command(vec![
        "release".to_owned(),
        "resume".to_owned(),
        "--allow-stale".to_owned(),
    ])
    .expect_err("the prepared-state resume is retired");
    assert!(error.to_string().contains("--allow-stale"), "{error}");
}

#[test]
fn parse_release_resume_approve_and_receipt_take_a_run_id() {
    let resume = parse_command(vec![
        "release".to_owned(),
        "resume".to_owned(),
        "run-abc".to_owned(),
    ])
    .expect("resume parse");
    assert_eq!(
        resume,
        Command::Release(ReleaseArgs {
            subcommand: ReleaseSubcommand::ResumeRun {
                run: "run-abc".to_owned()
            },
            repo_override: None,
            output_json: false,
        })
    );

    let approve = parse_command(vec![
        "release".to_owned(),
        "approve".to_owned(),
        "run-abc".to_owned(),
    ])
    .expect("approve parse");
    assert_eq!(
        approve,
        Command::Release(ReleaseArgs {
            subcommand: ReleaseSubcommand::Approve {
                run: "run-abc".to_owned()
            },
            repo_override: None,
            output_json: false,
        })
    );

    let receipt =
        parse_command(vec!["release".to_owned(), "receipt".to_owned()]).expect("receipt parse");
    assert_eq!(
        receipt,
        Command::Release(ReleaseArgs {
            subcommand: ReleaseSubcommand::Receipt { run: None },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_release_prepare_and_execute_name_their_replacement() {
    for retired in ["prepare", "execute"] {
        let error = parse_command(vec!["release".to_owned(), retired.to_owned()])
            .expect_err("the retired subcommand refuses");
        let message = error.to_string();
        assert!(message.contains("retired in v0.15"), "{message}");
        assert!(message.contains("effigy release run"), "{message}");
    }
}

#[test]
fn parse_release_verify_install_with_repo_tag_and_repo_url() {
    let cmd = parse_command(vec![
        "release".to_owned(),
        "verify-install".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--tag".to_owned(),
        "v0.2.5".to_owned(),
        "--repo-url".to_owned(),
        "https://example.com/repo.git".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Release(ReleaseArgs {
            subcommand: ReleaseSubcommand::VerifyInstall {
                tag: Some("v0.2.5".to_owned()),
                repo_url: Some("https://example.com/repo.git".to_owned()),
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_release_plan_with_repo() {
    let cmd = parse_command(vec![
        "release".to_owned(),
        "--plan".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Release(ReleaseArgs {
            subcommand: ReleaseSubcommand::Plan {
                version_override: None,
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_release_plan_with_version_override_and_dry_run_alias() {
    let cmd = parse_command(vec![
        "release".to_owned(),
        "--plan".to_owned(),
        "--version".to_owned(),
        "0.2.8".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Release(ReleaseArgs {
            subcommand: ReleaseSubcommand::Plan {
                version_override: Some("0.2.8".to_owned()),
            },
            repo_override: None,
            output_json: true,
        })
    );

    let alias = parse_command(vec![
        "release".to_owned(),
        "--dry-run".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        alias,
        Command::Release(ReleaseArgs {
            subcommand: ReleaseSubcommand::Plan {
                version_override: None,
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_release_plan_replaces_the_retired_simulate_command() {
    let error = parse_command(vec!["release".to_owned(), "simulate".to_owned()])
        .expect_err("release simulate must be refused");
    let message = format!("{error:?}");
    assert!(message.contains("retired"), "{message}");
    assert!(message.contains("effigy release --plan"), "{message}");
}
