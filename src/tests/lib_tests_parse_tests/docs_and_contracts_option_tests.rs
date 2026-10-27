use crate::tests::prelude::{
    parse_command, Command, ContainerArgs, ContainerSubcommand, ContractsArgs,
    ContractsSelectionPrintMode, ContractsSubcommand, DocsArgs, DocsBlockRequirement,
    DocsSubcommand, PathBuf,
};
use effigy_cli::DocsCheckKind;
use effigy_cli::{ChangelogArgs, ChangelogSubcommand};

#[test]
fn parse_docs_check_links_with_repo_and_paths() {
    let cmd = parse_command(vec![
        "docs".to_owned(),
        "check".to_owned(),
        "links".to_owned(),
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
            subcommand: DocsSubcommand::Check {
                kind: DocsCheckKind::Links,
                paths: vec![
                    PathBuf::from("README.md"),
                    PathBuf::from("docs/guides/README.md"),
                ],
                file: None,
                section: None,
                min_blocks: None,
                required_text: Vec::new(),
                required_blocks: Vec::new(),
                required_headings: Vec::new(),
                forbidden_text: Vec::new(),
                policy_index: Box::new(None),
                dir: Box::new(None),
                index: Box::new(None),
                policy_name: Box::new(None),
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
        "check".to_owned(),
        "json-examples".to_owned(),
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
            subcommand: DocsSubcommand::Check {
                kind: DocsCheckKind::JsonExamples,
                paths: Vec::new(),
                file: Some(PathBuf::from("docs/guides/examples.md")),
                section: Some("Examples".to_owned()),
                min_blocks: Some(3),
                required_text: vec!["\"schema\": \"example\"".to_owned()],
                required_blocks: vec![DocsBlockRequirement {
                    block_index: 2,
                    needle: "\"cache_state\": \"miss\"".to_owned(),
                }],
                required_headings: Vec::new(),
                forbidden_text: Vec::new(),
                policy_index: Box::new(None),
                dir: Box::new(None),
                index: Box::new(None),
                policy_name: Box::new(None),
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
        "check".to_owned(),
        "index".to_owned(),
        "--policy-index".to_owned(),
        "vision".to_owned(),
        "--dir".to_owned(),
        "docs/notes".to_owned(),
        "--index".to_owned(),
        "docs/notes/README.md".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Docs(DocsArgs {
            subcommand: DocsSubcommand::Check {
                kind: DocsCheckKind::Index,
                paths: Vec::new(),
                file: None,
                section: None,
                min_blocks: None,
                required_text: Vec::new(),
                required_blocks: Vec::new(),
                required_headings: Vec::new(),
                forbidden_text: Vec::new(),
                policy_index: Box::new(Some("vision".to_owned())),
                dir: Box::new(Some(PathBuf::from("docs/notes"))),
                index: Box::new(Some(PathBuf::from("docs/notes/README.md"))),
                policy_name: Box::new(None),
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
        "docs/notes/2026-03/02-160000-my-log.md".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Docs(DocsArgs {
            subcommand: DocsSubcommand::AddLogIndex {
                log_path: PathBuf::from("docs/notes/2026-03/02-160000-my-log.md"),
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
        "check".to_owned(),
        "next-action".to_owned(),
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
            subcommand: DocsSubcommand::Check {
                kind: DocsCheckKind::NextAction,
                paths: Vec::new(),
                file: None,
                section: None,
                min_blocks: None,
                required_text: Vec::new(),
                required_blocks: Vec::new(),
                required_headings: Vec::new(),
                forbidden_text: Vec::new(),
                policy_index: Box::new(None),
                dir: Box::new(None),
                index: Box::new(None),
                policy_name: Box::new(Some("vision".to_owned())),
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
        "check".to_owned(),
        "forbidden".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "AGENTS.md".to_owned(),
        "external/setup-effigy/README.md".to_owned(),
        "--forbid".to_owned(),
        "--repo .".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Docs(DocsArgs {
            subcommand: DocsSubcommand::Check {
                kind: DocsCheckKind::Forbidden,
                paths: vec![
                    PathBuf::from("AGENTS.md"),
                    PathBuf::from("external/setup-effigy/README.md"),
                ],
                file: None,
                section: None,
                min_blocks: None,
                required_text: Vec::new(),
                required_blocks: Vec::new(),
                required_headings: Vec::new(),
                forbidden_text: vec!["--repo .".to_owned()],
                policy_index: Box::new(None),
                dir: Box::new(None),
                index: Box::new(None),
                policy_name: Box::new(None),
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_changelog_validate_supports_repo_override() {
    let cmd = parse_command(vec![
        "changelog".to_owned(),
        "validate".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Changelog(ChangelogArgs {
            subcommand: ChangelogSubcommand::Validate,
            file: None,
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_docs_check_headings_with_requirements() {
    let cmd = parse_command(vec![
        "docs".to_owned(),
        "check".to_owned(),
        "headings".to_owned(),
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
            subcommand: DocsSubcommand::Check {
                kind: DocsCheckKind::Headings,
                paths: vec![PathBuf::from(
                    "docs/guides/024-ci-and-automation-recipes.md"
                )],
                file: None,
                section: None,
                min_blocks: None,
                required_text: Vec::new(),
                required_blocks: Vec::new(),
                required_headings: vec!["## Vision Alignment".to_owned()],
                forbidden_text: Vec::new(),
                policy_index: Box::new(None),
                dir: Box::new(None),
                index: Box::new(None),
                policy_name: Box::new(None),
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
        "check".to_owned(),
        "paths".to_owned(),
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
            subcommand: DocsSubcommand::Check {
                kind: DocsCheckKind::Paths,
                paths: vec![PathBuf::from("README.md"), PathBuf::from("docs/README.md")],
                file: None,
                section: None,
                min_blocks: None,
                required_text: Vec::new(),
                required_blocks: Vec::new(),
                required_headings: Vec::new(),
                forbidden_text: Vec::new(),
                policy_index: Box::new(None),
                dir: Box::new(None),
                index: Box::new(None),
                policy_name: Box::new(None),
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
        "check".to_owned(),
        "workflow-paths".to_owned(),
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
            subcommand: DocsSubcommand::Check {
                kind: DocsCheckKind::WorkflowPaths,
                paths: Vec::new(),
                file: None,
                section: None,
                min_blocks: None,
                required_text: Vec::new(),
                required_blocks: Vec::new(),
                required_headings: Vec::new(),
                forbidden_text: Vec::new(),
                policy_index: Box::new(None),
                dir: Box::new(Some(PathBuf::from("docs/guides"))),
                index: Box::new(None),
                policy_name: Box::new(None),
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_docs_removed_flat_check_spelling_reports_unknown_argument() {
    let error = parse_command(vec!["docs".to_owned(), "check-links".to_owned()])
        .expect_err("old docs check spelling should fail");

    assert_eq!(error.to_string(), "unknown argument: check-links");
}

#[test]
fn parse_docs_context_is_retired_with_an_actionable_message() {
    let error = parse_command(vec![
        "docs".to_owned(),
        "context".to_owned(),
        "release gates".to_owned(),
    ])
    .expect_err("retired docs context must fail");
    let message = error.to_string();
    assert!(message.contains("was removed in v0.15"), "{message}");
    assert!(message.contains("docs/README.md"), "{message}");
    assert!(message.contains("effigy docs check"), "{message}");
    assert!(message.contains("CHANGELOG.md"), "{message}");
}

#[test]
fn parse_contracts_validate_selection_is_retired_with_its_replacement_named() {
    let error = parse_command(vec![
        "contracts".to_owned(),
        "validate-selection".to_owned(),
        "--artifact".to_owned(),
        "tmp/selected.json".to_owned(),
    ])
    .expect_err("retired subcommand must be refused");

    let message = error.to_string();
    assert!(message.contains("validate-selection"), "{message}");
    assert!(
        message.contains("check-json --validate-selection"),
        "{message}"
    );
    assert!(message.contains("--artifact"), "{message}");
    assert!(message.contains("CHANGELOG.md"), "{message}");
}

#[test]
fn parse_contracts_check_json_with_selection_validation() {
    let cmd = parse_command(vec![
        "contracts".to_owned(),
        "check-json".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--contract".to_owned(),
        "docs/knowledge/contracts/selection.json".to_owned(),
        "--validate-selection".to_owned(),
        "--artifact".to_owned(),
        "tmp/selected.json".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Contracts(ContractsArgs {
            subcommand: ContractsSubcommand::CheckJson {
                index_path: None,
                contract_path: Some(PathBuf::from("docs/knowledge/contracts/selection.json")),
                artifact_path: Some(PathBuf::from("tmp/selected.json")),
                changed_only_base: None,
                print_selected: ContractsSelectionPrintMode::None,
                validate_selection: true,
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
        "docs/knowledge/contracts/index.json".to_owned(),
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
                index_path: Some(PathBuf::from("docs/knowledge/contracts/index.json")),
                contract_path: None,
                artifact_path: None,
                changed_only_base: Some("origin/main".to_owned()),
                print_selected: ContractsSelectionPrintMode::Json,
                validate_selection: false,
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
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
                system: None,
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
                system: None,
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
                bounded: None,
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: false,
        })
    );
}
