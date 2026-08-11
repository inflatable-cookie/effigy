use crate::tests::prelude::{parse_command, Command, DepsArgs, DepsManager, DepsSubcommand};
use effigy_cli::{apply_global_cli_flags, GlobalCliOptions};
use std::path::{Path, PathBuf};

#[test]
fn bare_deps_and_explicit_status_parse_equivalently() {
    let bare = parse_command(["deps".to_owned()]).unwrap();
    let explicit = parse_command(["deps".to_owned(), "status".to_owned()]).unwrap();

    assert_eq!(bare, explicit);
    assert_eq!(
        bare,
        Command::Deps(DepsArgs {
            subcommand: DepsSubcommand::Status { manager: None },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn deps_status_parses_manager_repo_and_json() {
    let parsed = parse_command([
        "deps".to_owned(),
        "status".to_owned(),
        "cargo".to_owned(),
        "--repo".to_owned(),
        "/tmp/consumer".to_owned(),
        "--json".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        parsed,
        Command::Deps(DepsArgs {
            subcommand: DepsSubcommand::Status {
                manager: Some(DepsManager::Cargo),
            },
            repo_override: Some(PathBuf::from("/tmp/consumer")),
            output_json: true,
        })
    );
}

#[test]
fn deps_link_and_unlink_capture_manager_path_and_dry_run() {
    let link = parse_command([
        "deps".to_owned(),
        "link".to_owned(),
        "cargo".to_owned(),
        "../signal".to_owned(),
        "--dry-run".to_owned(),
    ])
    .unwrap();
    let unlink = parse_command([
        "deps".to_owned(),
        "unlink".to_owned(),
        "bun".to_owned(),
        "../poodle".to_owned(),
    ])
    .unwrap();

    assert!(matches!(
        link,
        Command::Deps(DepsArgs {
            subcommand: DepsSubcommand::Link {
                manager: DepsManager::Cargo,
                library_path,
                dry_run: true,
            },
            ..
        }) if library_path == Path::new("../signal")
    ));
    assert!(matches!(
        unlink,
        Command::Deps(DepsArgs {
            subcommand: DepsSubcommand::Unlink {
                manager: DepsManager::Bun,
                library_path,
                dry_run: false,
            },
            ..
        }) if library_path == Path::new("../poodle")
    ));
}

#[test]
fn deps_pin_and_unpin_capture_manager_path_and_dry_run() {
    let pin = parse_command([
        "deps".to_owned(),
        "pin".to_owned(),
        "bun".to_owned(),
        "../poodle".to_owned(),
        "--dry-run".to_owned(),
    ])
    .unwrap();
    let unpin = parse_command([
        "deps".to_owned(),
        "unpin".to_owned(),
        "cargo".to_owned(),
        "../poodle".to_owned(),
    ])
    .unwrap();

    assert!(matches!(
        pin,
        Command::Deps(DepsArgs {
            subcommand: DepsSubcommand::Pin {
                manager: DepsManager::Bun,
                library_path,
                dry_run: true,
            },
            ..
        }) if library_path == Path::new("../poodle")
    ));
    assert!(matches!(
        unpin,
        Command::Deps(DepsArgs {
            subcommand: DepsSubcommand::Unpin {
                manager: DepsManager::Cargo,
                library_path,
                dry_run: false,
            },
            ..
        }) if library_path == Path::new("../poodle")
    ));
}

#[test]
fn deps_rejects_unknown_manager_and_status_dry_run() {
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
