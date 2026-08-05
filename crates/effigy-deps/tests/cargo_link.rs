use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use effigy_deps::{
    execute_cargo_link, execute_cargo_unlink, CargoLinkOutcome, CargoLockfileState,
    CargoUnlinkOutcome, RepoLinkStateStore, StdReadOnlyProcess, VerificationStatus,
};
use tempfile::TempDir;

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

fn run(cwd: &Path, program: &str, args: &[&str]) -> String {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|error| panic!("failed to run {program}: {error}"));
    assert!(
        output.status.success(),
        "{program} {} failed in {}:\n{}",
        args.join(" "),
        cwd.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn create_library() -> TempDir {
    create_named_library("effigy-link-fixture-core", "effigy-link-fixture-protocol")
}

fn create_named_library(core: &str, protocol: &str) -> TempDir {
    let library = TempDir::new().unwrap();
    write(
        &library.path().join("Cargo.toml"),
        "[workspace]\nmembers=['crates/core','crates/protocol']\nresolver='2'\n",
    );
    write(
        &library.path().join("crates/protocol/Cargo.toml"),
        &format!("[package]\nname='{protocol}'\nversion='0.1.0'\nedition='2021'\n"),
    );
    write(
        &library.path().join("crates/protocol/src/lib.rs"),
        "pub fn value() -> u8 { 42 }\n",
    );
    write(
        &library.path().join("crates/core/Cargo.toml"),
        &format!(
            "[package]\nname='{core}'\nversion='0.1.0'\nedition='2021'\n[dependencies]\n{protocol}={{path='../protocol'}}\n"
        ),
    );
    write(
        &library.path().join("crates/core/src/lib.rs"),
        &format!(
            "pub const LINK_PROBE: u8 = 1;\npub fn value() -> u8 {{ {}::value() }}\n",
            protocol.replace('-', "_")
        ),
    );
    run(library.path(), "git", &["init", "-q"]);
    run(
        library.path(),
        "git",
        &["config", "user.email", "effigy-fixture@example.test"],
    );
    run(
        library.path(),
        "git",
        &["config", "user.name", "Effigy Fixture"],
    );
    run(library.path(), "git", &["add", "."]);
    run(library.path(), "git", &["commit", "-qm", "fixture"]);
    run(library.path(), "git", &["tag", "v0.1.0"]);
    library
}

fn consumer_manifest(name: &str, git_url: &str) -> String {
    format!(
        "[package]\nname='{name}'\nversion='0.1.0'\nedition='2021'\n[dependencies]\neffigy-link-fixture-core={{git='{git_url}',tag='v0.1.0'}}\n"
    )
}

fn prepare_consumer(repo: &Path, roots: &[PathBuf], git_url: &str) {
    for (index, root) in roots.iter().enumerate() {
        write(
            &root.join("Cargo.toml"),
            &consumer_manifest(&format!("effigy-link-consumer-{index}"), git_url),
        );
        write(
            &root.join("src/lib.rs"),
            "pub fn consumer() -> u8 { effigy_link_fixture_core::value() }\n",
        );
        run(
            repo,
            "cargo",
            &[
                "generate-lockfile",
                "--manifest-path",
                root.join("Cargo.toml").to_str().unwrap(),
            ],
        );
    }
    run(repo, "git", &["init", "-q"]);
    run(
        repo,
        "git",
        &["config", "user.email", "effigy-fixture@example.test"],
    );
    run(repo, "git", &["config", "user.name", "Effigy Fixture"]);
    run(repo, "git", &["add", "."]);
    run(repo, "git", &["commit", "-qm", "consumer fixture"]);
}

fn assert_real_round_trip(roots: Vec<PathBuf>) {
    let library = create_library();
    let library_root = fs::canonicalize(library.path()).unwrap();
    let git_url = format!("file://{}", library_root.display());
    let repo_temp = TempDir::new().unwrap();
    let repo = fs::canonicalize(repo_temp.path()).unwrap();
    let roots = roots
        .into_iter()
        .map(|root| repo.join(root))
        .collect::<Vec<_>>();
    prepare_consumer(&repo, &roots, &git_url);

    let report = execute_cargo_link(&repo, &library_root, false, &StdReadOnlyProcess).unwrap();

    assert_eq!(report.outcome, CargoLinkOutcome::Applied);
    assert_eq!(report.verification.status, VerificationStatus::Passed);
    assert_eq!(report.verification.evidence.len(), roots.len() * 2);
    assert!(report
        .verification
        .evidence
        .iter()
        .all(|evidence| evidence.observed_source == Some(evidence.expected_source.clone())));
    assert!(repo.join(".cargo/config.toml").exists());
    assert_eq!(
        RepoLinkStateStore::for_repo(&repo)
            .read()
            .unwrap()
            .links
            .len(),
        1
    );

    write(
        &library_root.join("crates/core/src/lib.rs"),
        "pub const LINK_PROBE: u8 = 2;\npub fn value() -> u8 { effigy_link_fixture_protocol::value() }\n",
    );
    for root in &roots {
        write(
            &root.join("src/lib.rs"),
            "const _: [(); 2] = [(); effigy_link_fixture_core::LINK_PROBE as usize];\npub fn consumer() -> u8 { effigy_link_fixture_core::value() }\n",
        );
        run(
            &repo,
            "cargo",
            &[
                "check",
                "--manifest-path",
                root.join("Cargo.toml").to_str().unwrap(),
            ],
        );
        write(
            &root.join("src/lib.rs"),
            "pub fn consumer() -> u8 { effigy_link_fixture_core::value() }\n",
        );
    }

    let unlink = execute_cargo_unlink(&repo, &library_root, false, &StdReadOnlyProcess).unwrap();
    assert_eq!(unlink.outcome, CargoUnlinkOutcome::Unlinked, "{unlink:#?}");
    assert_eq!(unlink.verification.status, VerificationStatus::Passed);
    assert!(unlink
        .lockfiles
        .iter()
        .all(|lock| lock.after_state == CargoLockfileState::Clean));
    assert!(!repo.join(".cargo/config.toml").exists());
    assert!(!RepoLinkStateStore::for_repo(&repo).path().exists());
    for root in &roots {
        run(
            &repo,
            "cargo",
            &[
                "check",
                "--manifest-path",
                root.join("Cargo.toml").to_str().unwrap(),
            ],
        );
        let status = run(
            &repo,
            "git",
            &[
                "status",
                "--porcelain=v1",
                "--",
                root.join("Cargo.lock").to_str().unwrap(),
            ],
        );
        assert!(status.is_empty(), "Cargo.lock remained dirty: {status}");
    }
}

#[test]
fn real_flat_git_dependency_resolves_the_full_local_closure() {
    assert_real_round_trip(vec![PathBuf::new()]);
}

#[test]
fn real_nested_git_dependencies_share_the_repo_root_patch_and_verify_per_workspace() {
    assert_real_round_trip(vec![PathBuf::from("apps/one"), PathBuf::from("apps/two")]);
}

#[test]
fn unlink_preserves_foreign_cargo_state_and_another_active_library() {
    let first = create_library();
    let second = create_named_library("effigy-link-other-core", "effigy-link-other-protocol");
    let first_root = fs::canonicalize(first.path()).unwrap();
    let second_root = fs::canonicalize(second.path()).unwrap();
    let first_url = format!("file://{}", first_root.display());
    let second_url = format!("file://{}", second_root.display());
    let repo_temp = TempDir::new().unwrap();
    let repo = fs::canonicalize(repo_temp.path()).unwrap();
    write(
        &repo.join("Cargo.toml"),
        &format!(
            "[package]\nname='effigy-link-two-libraries'\nversion='0.1.0'\nedition='2021'\n[dependencies]\neffigy-link-fixture-core={{git='{first_url}',tag='v0.1.0'}}\neffigy-link-other-core={{git='{second_url}',tag='v0.1.0'}}\n"
        ),
    );
    write(&repo.join("src/lib.rs"), "pub fn consumer() {}\n");
    write(&repo.join(".gitignore"), ".cargo/config.toml\n.effigy/\n");
    run(&repo, "cargo", &["generate-lockfile"]);
    run(&repo, "git", &["init", "-q"]);
    run(
        &repo,
        "git",
        &["config", "user.email", "effigy-fixture@example.test"],
    );
    run(&repo, "git", &["config", "user.name", "Effigy Fixture"]);
    run(&repo, "git", &["add", "."]);
    run(&repo, "git", &["commit", "-qm", "consumer fixture"]);
    let foreign_config = "# foreign cargo config\n[net]\ngit-fetch-with-cli = true\n";
    let foreign_credentials = "[registry]\ntoken = 'fixture'\n";
    write(&repo.join(".cargo/config.toml"), foreign_config);
    write(&repo.join(".cargo/credentials.toml"), foreign_credentials);

    let first_link = execute_cargo_link(&repo, &first_root, false, &StdReadOnlyProcess).unwrap();
    let second_link = execute_cargo_link(&repo, &second_root, false, &StdReadOnlyProcess).unwrap();
    assert_eq!(
        first_link.outcome,
        CargoLinkOutcome::Applied,
        "{first_link:#?}"
    );
    assert_eq!(
        second_link.outcome,
        CargoLinkOutcome::Applied,
        "{second_link:#?}"
    );
    let before = fs::read_to_string(repo.join(".cargo/config.toml")).unwrap();
    let second_marker = format!("# >>> effigy deps cargo {} >>>", second_root.display());
    let second_block_start = before.find(&second_marker).unwrap();
    let second_block = before[second_block_start..].to_owned();

    let unlink = execute_cargo_unlink(&repo, &first_root, false, &StdReadOnlyProcess).unwrap();

    assert_eq!(unlink.outcome, CargoUnlinkOutcome::Unlinked);
    assert_eq!(unlink.verification.status, VerificationStatus::Passed);
    assert!(unlink
        .lockfiles
        .iter()
        .all(|lock| lock.after_state == CargoLockfileState::ActiveLinks));
    let after = fs::read_to_string(repo.join(".cargo/config.toml")).unwrap();
    assert!(after.starts_with(foreign_config));
    assert!(after.ends_with(&second_block));
    assert_eq!(
        fs::read_to_string(repo.join(".cargo/credentials.toml")).unwrap(),
        foreign_credentials
    );
    let state = RepoLinkStateStore::for_repo(&repo).read().unwrap();
    assert_eq!(state.links.len(), 1);
    assert_eq!(state.links[0].key.library_path, second_root);

    let config_before_refusal = fs::read_to_string(repo.join(".cargo/config.toml")).unwrap();
    let state_before_refusal =
        fs::read_to_string(RepoLinkStateStore::for_repo(&repo).path()).unwrap();
    let lock_path = repo.join("Cargo.lock");
    let lock = fs::read_to_string(&lock_path).unwrap();
    assert!(lock.contains("version = 4"));
    write(&lock_path, &lock.replacen("version = 4", "version = 3", 1));

    let refused = execute_cargo_unlink(&repo, &second_root, false, &StdReadOnlyProcess).unwrap();
    assert_eq!(refused.outcome, CargoUnlinkOutcome::VerificationFailed);
    assert!(refused.applied_files.is_empty());
    assert_eq!(
        refused.lockfiles[0].before_state,
        CargoLockfileState::UnexpectedDrift
    );
    assert_eq!(
        fs::read_to_string(repo.join(".cargo/config.toml")).unwrap(),
        config_before_refusal
    );
    assert_eq!(
        fs::read_to_string(RepoLinkStateStore::for_repo(&repo).path()).unwrap(),
        state_before_refusal
    );
}
