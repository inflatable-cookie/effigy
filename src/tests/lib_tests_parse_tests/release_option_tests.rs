use super::prelude::{parse_command, Command, PathBuf, ReleaseArgs, ReleaseSubcommand};

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
fn parse_release_resume_with_repo_and_allow_stale() {
    let cmd = parse_command(vec![
        "release".to_owned(),
        "resume".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--allow-stale".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Release(ReleaseArgs {
            subcommand: ReleaseSubcommand::Resume { allow_stale: true },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
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
fn parse_release_simulate_with_repo() {
    let cmd = parse_command(vec![
        "release".to_owned(),
        "simulate".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Release(ReleaseArgs {
            subcommand: ReleaseSubcommand::Simulate {
                version_override: None,
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_release_simulate_with_version_override() {
    let cmd = parse_command(vec![
        "release".to_owned(),
        "simulate".to_owned(),
        "--version".to_owned(),
        "0.2.8".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Release(ReleaseArgs {
            subcommand: ReleaseSubcommand::Simulate {
                version_override: Some("0.2.8".to_owned()),
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_release_prepare_plan_with_repo_and_gate_check() {
    let cmd = parse_command(vec![
        "release".to_owned(),
        "prepare".to_owned(),
        "--plan".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--check-gates".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Release(ReleaseArgs {
            subcommand: ReleaseSubcommand::Prepare {
                plan: true,
                check_gates: true,
                yes: false,
                version_override: None,
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_release_prepare_dry_run_alias_with_repo_and_gate_check() {
    let cmd = parse_command(vec![
        "release".to_owned(),
        "prepare".to_owned(),
        "--dry-run".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--check-gates".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Release(ReleaseArgs {
            subcommand: ReleaseSubcommand::Prepare {
                plan: true,
                check_gates: true,
                yes: false,
                version_override: None,
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_release_prepare_yes_with_repo_and_gate_check() {
    let cmd = parse_command(vec![
        "release".to_owned(),
        "prepare".to_owned(),
        "--yes".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--check-gates".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Release(ReleaseArgs {
            subcommand: ReleaseSubcommand::Prepare {
                plan: false,
                check_gates: true,
                yes: true,
                version_override: None,
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_release_prepare_plan_with_version_override() {
    let cmd = parse_command(vec![
        "release".to_owned(),
        "prepare".to_owned(),
        "--plan".to_owned(),
        "--version".to_owned(),
        "0.2.8".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Release(ReleaseArgs {
            subcommand: ReleaseSubcommand::Prepare {
                plan: true,
                check_gates: false,
                yes: false,
                version_override: Some("0.2.8".to_owned()),
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_release_execute_plan_with_repo() {
    let cmd = parse_command(vec![
        "release".to_owned(),
        "execute".to_owned(),
        "--plan".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Release(ReleaseArgs {
            subcommand: ReleaseSubcommand::Execute {
                plan: true,
                yes: false,
                allow_stale: false,
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_release_execute_dry_run_alias_with_repo() {
    let cmd = parse_command(vec![
        "release".to_owned(),
        "execute".to_owned(),
        "--dry-run".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Release(ReleaseArgs {
            subcommand: ReleaseSubcommand::Execute {
                plan: true,
                yes: false,
                allow_stale: false,
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_release_execute_yes_with_repo() {
    let cmd = parse_command(vec![
        "release".to_owned(),
        "execute".to_owned(),
        "--yes".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Release(ReleaseArgs {
            subcommand: ReleaseSubcommand::Execute {
                plan: false,
                yes: true,
                allow_stale: false,
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_release_execute_allow_stale_with_repo() {
    let cmd = parse_command(vec![
        "release".to_owned(),
        "execute".to_owned(),
        "--allow-stale".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Release(ReleaseArgs {
            subcommand: ReleaseSubcommand::Execute {
                plan: false,
                yes: false,
                allow_stale: true,
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: false,
        })
    );
}
