use crate::tests::prelude::{
    parse_command, Command, ContractsArgs, ContractsSelectionPrintMode, ContractsSubcommand,
    DocsArgs, DocsBlockRequirement, DocsSubcommand, PathBuf,
};
use effigy_cli::DocsCheckKind;
use effigy_cli::{ChangelogArgs, ChangelogSubcommand};

/// What varies across `docs check <kind>` rows; `expected_shape` fills the
/// shared empty vocabulary so every row still asserts the full `DocsArgs`.
#[derive(Default, Clone)]
struct CheckShape {
    kind: Option<DocsCheckKind>,
    paths: Vec<PathBuf>,
    file: Option<PathBuf>,
    section: Option<String>,
    min_blocks: Option<usize>,
    required_text: Vec<String>,
    required_blocks: Vec<DocsBlockRequirement>,
    required_headings: Vec<String>,
    forbidden_text: Vec<String>,
    policy_index: Option<String>,
    dir: Option<PathBuf>,
    index: Option<PathBuf>,
    policy_name: Option<String>,
    repo_override: Option<PathBuf>,
    output_json: bool,
}

fn expected_shape(shape: CheckShape) -> Command {
    Command::Docs(DocsArgs {
        subcommand: DocsSubcommand::Check {
            kind: shape.kind.expect("every row names its docs check kind"),
            paths: shape.paths,
            file: shape.file,
            section: shape.section,
            min_blocks: shape.min_blocks,
            required_text: shape.required_text,
            required_blocks: shape.required_blocks,
            required_headings: shape.required_headings,
            forbidden_text: shape.forbidden_text,
            policy_index: Box::new(shape.policy_index),
            dir: Box::new(shape.dir),
            index: Box::new(shape.index),
            policy_name: Box::new(shape.policy_name),
        },
        repo_override: shape.repo_override,
        output_json: shape.output_json,
    })
}

/// One row per `docs check <kind>` mapping: each row pins the kind and its
/// own flag vocabulary (paths, json-examples, index, next-action, forbidden,
/// headings, paths, workflow-paths) with full-struct equality.
#[test]
fn parse_docs_check_kinds_map_their_flag_vocabulary() {
    let cases: &[(&[&str], CheckShape)] = &[
        (
            &[
                "docs",
                "check",
                "links",
                "--repo",
                "/tmp/repo",
                "README.md",
                "docs/guides/README.md",
                "--json",
            ],
            CheckShape {
                kind: Some(DocsCheckKind::Links),
                paths: vec![
                    PathBuf::from("README.md"),
                    PathBuf::from("docs/guides/README.md"),
                ],
                repo_override: Some(PathBuf::from("/tmp/repo")),
                output_json: true,
                ..CheckShape::default()
            },
        ),
        (
            &[
                "docs",
                "check",
                "json-examples",
                "--repo",
                "/tmp/repo",
                "--file",
                "docs/guides/examples.md",
                "--section",
                "Examples",
                "--min-blocks",
                "3",
                "--require",
                "\"schema\": \"example\"",
                "--require-block",
                "2:\"cache_state\": \"miss\"",
                "--json",
            ],
            CheckShape {
                kind: Some(DocsCheckKind::JsonExamples),
                file: Some(PathBuf::from("docs/guides/examples.md")),
                section: Some("Examples".to_owned()),
                min_blocks: Some(3),
                required_text: vec!["\"schema\": \"example\"".to_owned()],
                required_blocks: vec![DocsBlockRequirement {
                    block_index: 2,
                    needle: "\"cache_state\": \"miss\"".to_owned(),
                }],
                repo_override: Some(PathBuf::from("/tmp/repo")),
                output_json: true,
                ..CheckShape::default()
            },
        ),
        (
            &[
                "docs",
                "check",
                "index",
                "--policy-index",
                "vision",
                "--dir",
                "docs/notes",
                "--index",
                "docs/notes/README.md",
                "--json",
            ],
            CheckShape {
                kind: Some(DocsCheckKind::Index),
                policy_index: Some("vision".to_owned()),
                dir: Some(PathBuf::from("docs/notes")),
                index: Some(PathBuf::from("docs/notes/README.md")),
                output_json: true,
                ..CheckShape::default()
            },
        ),
        (
            &[
                "docs",
                "check",
                "next-action",
                "--repo",
                "/tmp/repo",
                "--policy",
                "vision",
                "--json",
            ],
            CheckShape {
                kind: Some(DocsCheckKind::NextAction),
                policy_name: Some("vision".to_owned()),
                repo_override: Some(PathBuf::from("/tmp/repo")),
                output_json: true,
                ..CheckShape::default()
            },
        ),
        (
            &[
                "docs",
                "check",
                "forbidden",
                "--repo",
                "/tmp/repo",
                "AGENTS.md",
                "external/setup-effigy/README.md",
                "--forbid",
                "--repo .",
                "--json",
            ],
            CheckShape {
                kind: Some(DocsCheckKind::Forbidden),
                paths: vec![
                    PathBuf::from("AGENTS.md"),
                    PathBuf::from("external/setup-effigy/README.md"),
                ],
                forbidden_text: vec!["--repo .".to_owned()],
                repo_override: Some(PathBuf::from("/tmp/repo")),
                output_json: true,
                ..CheckShape::default()
            },
        ),
        (
            &[
                "docs",
                "check",
                "headings",
                "--repo",
                "/tmp/repo",
                "docs/guides/024-ci-and-automation-recipes.md",
                "--require-heading",
                "## Vision Alignment",
                "--json",
            ],
            CheckShape {
                kind: Some(DocsCheckKind::Headings),
                paths: vec![PathBuf::from(
                    "docs/guides/024-ci-and-automation-recipes.md",
                )],
                required_headings: vec!["## Vision Alignment".to_owned()],
                repo_override: Some(PathBuf::from("/tmp/repo")),
                output_json: true,
                ..CheckShape::default()
            },
        ),
        (
            &[
                "docs",
                "check",
                "paths",
                "--repo",
                "/tmp/repo",
                "README.md",
                "docs/README.md",
                "--json",
            ],
            CheckShape {
                kind: Some(DocsCheckKind::Paths),
                paths: vec![PathBuf::from("README.md"), PathBuf::from("docs/README.md")],
                repo_override: Some(PathBuf::from("/tmp/repo")),
                output_json: true,
                ..CheckShape::default()
            },
        ),
        (
            &[
                "docs",
                "check",
                "workflow-paths",
                "--repo",
                "/tmp/repo",
                "--dir",
                "docs/guides",
                "--json",
            ],
            CheckShape {
                kind: Some(DocsCheckKind::WorkflowPaths),
                dir: Some(PathBuf::from("docs/guides")),
                repo_override: Some(PathBuf::from("/tmp/repo")),
                output_json: true,
                ..CheckShape::default()
            },
        ),
    ];
    for (args, shape) in cases {
        assert_eq!(
            parse_command(args.iter().map(|arg| (*arg).to_owned())).expect("parse should succeed"),
            expected_shape(shape.clone()),
            "{args:?}"
        );
    }
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

/// The two `contracts check-json` flag clusters: selection validation
/// against an artifact, and changed-only scanning with selection printing.
#[test]
fn parse_contracts_check_json_maps_selection_and_changed_only_flags() {
    let cases: &[(&[&str], ContractsArgs)] = &[
        (
            &[
                "contracts",
                "check-json",
                "--repo",
                "/tmp/repo",
                "--contract",
                "docs/knowledge/contracts/selection.json",
                "--validate-selection",
                "--artifact",
                "tmp/selected.json",
                "--json",
            ],
            ContractsArgs {
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
            },
        ),
        (
            &[
                "contracts",
                "check-json",
                "--repo",
                "/tmp/repo",
                "--index",
                "docs/knowledge/contracts/index.json",
                "--changed-only",
                "origin/main",
                "--print-selected=json",
                "--json",
            ],
            ContractsArgs {
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
            },
        ),
    ];
    for (args, expected) in cases {
        assert_eq!(
            parse_command(args.iter().map(|arg| (*arg).to_owned())).expect("parse should succeed"),
            Command::Contracts(expected.clone()),
            "{args:?}"
        );
    }
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
