use crate::tests::prelude::{
    parse_command, Command, ContainerArgs, ContainerSubcommand, ContractsArgs, ContractsCheckMode,
    ContractsSelectionPrintMode, ContractsSubcommand, DistributionArgs, DistributionSubcommand,
    DocsArgs, DocsBlockRequirement, DocsSubcommand, PathBuf,
};

#[test]
fn parse_docs_check_links_with_repo_and_paths() {
    let cmd = parse_command(vec![
        "docs".to_owned(),
        "check-links".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "README.md".to_owned(),
        "docs/guides/README.md".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Docs(DocsArgs {
            subcommand: DocsSubcommand::CheckLinks {
                paths: vec![
                    PathBuf::from("README.md"),
                    PathBuf::from("docs/guides/README.md"),
                ],
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_docs_check_json_examples_with_overrides() {
    let cmd = parse_command(vec![
        "docs".to_owned(),
        "check-json-examples".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--file".to_owned(),
        "docs/guides/examples.md".to_owned(),
        "--section".to_owned(),
        "Examples".to_owned(),
        "--min-blocks".to_owned(),
        "3".to_owned(),
        "--require".to_owned(),
        "\"schema\": \"example\"".to_owned(),
        "--require-block".to_owned(),
        "2:\"cache_state\": \"miss\"".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Docs(DocsArgs {
            subcommand: DocsSubcommand::CheckJsonExamples {
                file: Some(PathBuf::from("docs/guides/examples.md")),
                section: Some("Examples".to_owned()),
                min_blocks: Some(3),
                required: vec!["\"schema\": \"example\"".to_owned()],
                required_blocks: vec![DocsBlockRequirement {
                    block_index: 2,
                    needle: "\"cache_state\": \"miss\"".to_owned(),
                }],
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_docs_check_index_with_overrides() {
    let cmd = parse_command(vec![
        "docs".to_owned(),
        "check-index".to_owned(),
        "--policy-index".to_owned(),
        "vision".to_owned(),
        "--dir".to_owned(),
        "docs/logs".to_owned(),
        "--index".to_owned(),
        "docs/logs/README.md".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Docs(DocsArgs {
            subcommand: DocsSubcommand::CheckIndex {
                policy_index: Some("vision".to_owned()),
                dir: Some(PathBuf::from("docs/logs")),
                index: Some(PathBuf::from("docs/logs/README.md")),
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_docs_add_log_index_with_repo_override() {
    let cmd = parse_command(vec![
        "docs".to_owned(),
        "add-log-index".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "docs/logs/2026-03/02-160000-my-log.md".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Docs(DocsArgs {
            subcommand: DocsSubcommand::AddLogIndex {
                log_path: PathBuf::from("docs/logs/2026-03/02-160000-my-log.md"),
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_docs_check_next_action_with_policy() {
    let cmd = parse_command(vec![
        "docs".to_owned(),
        "check-next-action".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--policy".to_owned(),
        "vision".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Docs(DocsArgs {
            subcommand: DocsSubcommand::CheckNextAction {
                policy_name: Some("vision".to_owned()),
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_docs_check_forbidden_with_requirements() {
    let cmd = parse_command(vec![
        "docs".to_owned(),
        "check-forbidden".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "AGENTS.md".to_owned(),
        "setup-effigy/README.md".to_owned(),
        "--forbid".to_owned(),
        "--repo .".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Docs(DocsArgs {
            subcommand: DocsSubcommand::CheckForbidden {
                paths: vec![
                    PathBuf::from("AGENTS.md"),
                    PathBuf::from("setup-effigy/README.md"),
                ],
                forbidden_text: vec!["--repo .".to_owned()],
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_docs_check_headings_with_requirements() {
    let cmd = parse_command(vec![
        "docs".to_owned(),
        "check-headings".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "docs/guides/024-ci-and-automation-recipes.md".to_owned(),
        "--require-heading".to_owned(),
        "## Vision Alignment".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Docs(DocsArgs {
            subcommand: DocsSubcommand::CheckHeadings {
                paths: vec![PathBuf::from(
                    "docs/guides/024-ci-and-automation-recipes.md"
                )],
                required_headings: vec!["## Vision Alignment".to_owned()],
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_docs_check_paths_with_repo_and_json() {
    let cmd = parse_command(vec![
        "docs".to_owned(),
        "check-paths".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "README.md".to_owned(),
        "docs/README.md".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Docs(DocsArgs {
            subcommand: DocsSubcommand::CheckPaths {
                paths: vec![PathBuf::from("README.md"), PathBuf::from("docs/README.md")],
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_docs_check_workflow_paths_with_dir_override() {
    let cmd = parse_command(vec![
        "docs".to_owned(),
        "check-workflow-paths".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--dir".to_owned(),
        "docs/guides".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Docs(DocsArgs {
            subcommand: DocsSubcommand::CheckWorkflowPaths {
                dir: Some(PathBuf::from("docs/guides")),
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_contracts_validate_selection_with_overrides() {
    let cmd = parse_command(vec![
        "contracts".to_owned(),
        "validate-selection".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--contract".to_owned(),
        "docs/contracts/selection.json".to_owned(),
        "--artifact".to_owned(),
        "tmp/selected.json".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Contracts(ContractsArgs {
            subcommand: ContractsSubcommand::ValidateSelection {
                contract_path: Some(PathBuf::from("docs/contracts/selection.json")),
                artifact_path: Some(PathBuf::from("tmp/selected.json")),
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_contracts_check_json_with_changed_only_and_selection_json() {
    let cmd = parse_command(vec![
        "contracts".to_owned(),
        "check-json".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--index".to_owned(),
        "docs/contracts/index.json".to_owned(),
        "--fast".to_owned(),
        "--changed-only".to_owned(),
        "origin/main".to_owned(),
        "--print-selected=json".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Contracts(ContractsArgs {
            subcommand: ContractsSubcommand::CheckJson {
                index_path: Some(PathBuf::from("docs/contracts/index.json")),
                mode: ContractsCheckMode::Fast,
                changed_only_base: Some("origin/main".to_owned()),
                print_selected: ContractsSelectionPrintMode::Json,
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_distribution_validate_metadata_with_tag() {
    let cmd = parse_command(vec![
        "distribution".to_owned(),
        "validate-metadata".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--tag".to_owned(),
        "v0.2.5".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Distribution(DistributionArgs {
            subcommand: DistributionSubcommand::ValidateMetadata {
                tag: Some("v0.2.5".to_owned()),
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_distribution_preflight_with_summary_output() {
    let cmd = parse_command(vec![
        "distribution".to_owned(),
        "preflight".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--tag".to_owned(),
        "v0.2.5".to_owned(),
        "--skip-smoke".to_owned(),
        "--output".to_owned(),
        "artifacts/preflight.env".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Distribution(DistributionArgs {
            subcommand: DistributionSubcommand::Preflight {
                tag: Some("v0.2.5".to_owned()),
                skip_docs: false,
                skip_smoke: true,
                output_path: Some(PathBuf::from("artifacts/preflight.env")),
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_distribution_check_glibc_floor_with_explicit_binary() {
    let cmd = parse_command(vec![
        "distribution".to_owned(),
        "check-glibc-floor".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--binary".to_owned(),
        "dist/effigy-linux".to_owned(),
        "--max-glibc".to_owned(),
        "2.35".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Distribution(DistributionArgs {
            subcommand: DistributionSubcommand::CheckGlibcFloor {
                binary_path: PathBuf::from("dist/effigy-linux"),
                max_glibc: "2.35".to_owned(),
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_distribution_first_publish_with_overrides() {
    let cmd = parse_command(vec![
        "distribution".to_owned(),
        "first-publish".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--tag".to_owned(),
        "v0.2.5".to_owned(),
        "--crate-version".to_owned(),
        "0.2.5".to_owned(),
        "--repo-url".to_owned(),
        "https://example.com/repo.git".to_owned(),
        "--brew-formula".to_owned(),
        "tap/effigy/effigy".to_owned(),
        "--skip-homebrew".to_owned(),
        "--artifacts-dir".to_owned(),
        "artifacts/dist".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Distribution(DistributionArgs {
            subcommand: DistributionSubcommand::FirstPublish {
                tag: "v0.2.5".to_owned(),
                crate_version: Some("0.2.5".to_owned()),
                repo_url: "https://example.com/repo.git".to_owned(),
                brew_formula: "tap/effigy/effigy".to_owned(),
                skip_homebrew: true,
                artifacts_dir: Some(PathBuf::from("artifacts/dist")),
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_distribution_validate_artifacts_with_homebrew() {
    let cmd = parse_command(vec![
        "distribution".to_owned(),
        "validate-artifacts".to_owned(),
        "--artifacts-dir".to_owned(),
        "artifacts/dist".to_owned(),
        "--expect-homebrew".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Distribution(DistributionArgs {
            subcommand: DistributionSubcommand::ValidateArtifacts {
                artifacts_dir: PathBuf::from("artifacts/dist"),
                expect_homebrew: true,
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_distribution_generate_closeout_with_output_and_owner() {
    let cmd = parse_command(vec![
        "distribution".to_owned(),
        "generate-closeout".to_owned(),
        "--tag".to_owned(),
        "v0.2.5".to_owned(),
        "--artifacts-dir".to_owned(),
        "artifacts/dist".to_owned(),
        "--output".to_owned(),
        "docs/logs/out.md".to_owned(),
        "--owner".to_owned(),
        "CI".to_owned(),
        "--expect-homebrew".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Distribution(DistributionArgs {
            subcommand: DistributionSubcommand::GenerateCloseout {
                tag: "v0.2.5".to_owned(),
                artifacts_dir: PathBuf::from("artifacts/dist"),
                output_path: Some(PathBuf::from("docs/logs/out.md")),
                owner: "CI".to_owned(),
                expect_homebrew: true,
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_distribution_write_summary_with_repeated_logs() {
    let cmd = parse_command(vec![
        "distribution".to_owned(),
        "write-summary".to_owned(),
        "--tag".to_owned(),
        "v0.2.5".to_owned(),
        "--artifacts-dir".to_owned(),
        "artifacts/dist".to_owned(),
        "--homebrew-executed".to_owned(),
        "--log-file".to_owned(),
        "01-tag-install-validation.log".to_owned(),
        "--log-file".to_owned(),
        "02-crates-io-install-validation.log".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Distribution(DistributionArgs {
            subcommand: DistributionSubcommand::WriteSummary {
                tag: "v0.2.5".to_owned(),
                artifacts_dir: PathBuf::from("artifacts/dist"),
                crate_version: None,
                repo_url: "https://github.com/inflatable-cookie/effigy.git".to_owned(),
                brew_formula: "inflatable-cookie/effigy/effigy".to_owned(),
                homebrew_executed: true,
                log_files: vec![
                    "01-tag-install-validation.log".to_owned(),
                    "02-crates-io-install-validation.log".to_owned()
                ],
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_container_up_without_name_uses_default_resolution_path() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "up".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--detach".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Up {
                name: None,
                attach: false,
                detach: true,
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_container_logs_with_explicit_name_and_service() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "web".to_owned(),
        "logs".to_owned(),
        "--service".to_owned(),
        "db".to_owned(),
        "--follow".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Logs {
                name: Some("web".to_owned()),
                service: Some("db".to_owned()),
                follow: true,
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_container_shell_with_command_override() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "web".to_owned(),
        "shell".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--command".to_owned(),
        "php artisan tinker".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Shell {
                name: Some("web".to_owned()),
                service: None,
                command: Some("php artisan tinker".to_owned()),
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: false,
        })
    );
}
