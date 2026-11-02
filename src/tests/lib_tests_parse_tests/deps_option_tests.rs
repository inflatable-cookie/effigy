use crate::tests::prelude::{parse_command, Command, DepsArgs, DepsManager, DepsSubcommand};
use effigy_cli::{apply_global_cli_flags, GlobalCliOptions};
use std::path::{Path, PathBuf};

/// One row per `deps` mapping: the bare `deps` default (identical to
/// `deps status`), the status manager/repo/json vocabulary and the four
/// mutation verbs with their library path and `--dry-run` join.
#[test]
fn parse_deps_surface_maps_managers_paths_and_dry_run() {
    let cases: &[(&[&str], Command)] = &[
        (
            &["deps"],
            Command::Deps(DepsArgs {
                subcommand: DepsSubcommand::Status { manager: None },
                repo_override: None,
                output_json: false,
            }),
        ),
        (
            &["deps", "status"],
            Command::Deps(DepsArgs {
                subcommand: DepsSubcommand::Status { manager: None },
                repo_override: None,
                output_json: false,
            }),
        ),
        (
            &[
                "deps",
                "status",
                "cargo",
                "--repo",
                "/tmp/consumer",
                "--json",
            ],
            Command::Deps(DepsArgs {
                subcommand: DepsSubcommand::Status {
                    manager: Some(DepsManager::Cargo),
                },
                repo_override: Some(PathBuf::from("/tmp/consumer")),
                output_json: true,
            }),
        ),
        (
            &["deps", "link", "cargo", "../signal", "--dry-run"],
            Command::Deps(DepsArgs {
                subcommand: DepsSubcommand::Link {
                    manager: DepsManager::Cargo,
                    library_path: PathBuf::from("../signal"),
                    dry_run: true,
                },
                repo_override: None,
                output_json: false,
            }),
        ),
        (
            &["deps", "unlink", "bun", "../poodle"],
            Command::Deps(DepsArgs {
                subcommand: DepsSubcommand::Unlink {
                    manager: DepsManager::Bun,
                    library_path: PathBuf::from("../poodle"),
                    dry_run: false,
                },
                repo_override: None,
                output_json: false,
            }),
        ),
        (
            &["deps", "pin", "bun", "../poodle", "--dry-run"],
            Command::Deps(DepsArgs {
                subcommand: DepsSubcommand::Pin {
                    manager: DepsManager::Bun,
                    library_path: PathBuf::from("../poodle"),
                    dry_run: true,
                },
                repo_override: None,
                output_json: false,
            }),
        ),
        (
            &["deps", "unpin", "cargo", "../poodle"],
            Command::Deps(DepsArgs {
                subcommand: DepsSubcommand::Unpin {
                    manager: DepsManager::Cargo,
                    library_path: PathBuf::from("../poodle"),
                    dry_run: false,
                },
                repo_override: None,
                output_json: false,
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
    // The bare default is the status route, not a distinct command.
    let bare = parse_command(["deps".to_owned()]).unwrap();
    let explicit = parse_command(["deps".to_owned(), "status".to_owned()]).unwrap();
    assert_eq!(bare, explicit);
}

/// Refusals keep their exact guidance: unknown package manager and
/// `--dry-run` on the read-only status route.
#[test]
fn parse_deps_refuses_unknown_manager_and_status_dry_run() {
    let manager_error = parse_command([
        "deps".to_owned(),
        "link".to_owned(),
        "npm".to_owned(),
        "../library".to_owned(),
    ])
    .unwrap_err();
    assert!(manager_error
        .to_string()
        .contains("expected `cargo` or `bun`"));

    let dry_run_error = parse_command([
        "deps".to_owned(),
        "status".to_owned(),
        "--dry-run".to_owned(),
    ])
    .unwrap_err();
    assert!(dry_run_error
        .to_string()
        .contains("accepted only by dependency mutation commands"));
}

/// Leading `--repo` and `--json` reach deps through the global
/// `get_or_insert` arms (`Command::Deps` in `apply_global_cli_options`).
#[test]
fn global_repo_and_json_flags_apply_to_deps() {
    let parsed = apply_global_cli_flags(
        parse_command(["deps".to_owned()]).unwrap(),
        &GlobalCliOptions {
            json_mode: true,
            repo_override: Some(PathBuf::from("/tmp/global-consumer")),
            ..GlobalCliOptions::default()
        },
    )
    .unwrap();

    assert!(matches!(
        parsed,
        Command::Deps(DepsArgs {
            repo_override: Some(path),
            output_json: true,
            ..
        }) if path == Path::new("/tmp/global-consumer")
    ));
}
