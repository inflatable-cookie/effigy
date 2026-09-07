use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use effigy_deps::{
    execute_cargo_link, execute_cargo_unlink, CargoLinkOutcome, CargoLockfileState,
    CargoUnlinkOutcome, CargoVersionTransition, RepoLinkStateStore, StdReadOnlyProcess,
    VerificationStatus,
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
    create_named_library(
        "effigy-link-fixture-core",
        "effigy-link-fixture-protocol",
        "0.1.0",
    )
}

fn create_named_library(core: &str, protocol: &str, version: &str) -> TempDir {
    let library = TempDir::new().unwrap();
    write(
        &library.path().join("Cargo.toml"),
        "[workspace]\nmembers=['crates/core','crates/protocol']\nresolver='2'\n",
    );
    write(
        &library.path().join("crates/protocol/Cargo.toml"),
        &format!("[package]\nname='{protocol}'\nversion='{version}'\nedition='2021'\n"),
    );
    write(
        &library.path().join("crates/protocol/src/lib.rs"),
        "pub fn value() -> u8 { 42 }\n",
    );
    write(
        &library.path().join("crates/core/Cargo.toml"),
        &format!(
            "[package]\nname='{core}'\nversion='{version}'\nedition='2021'\n[dependencies]\n{protocol}={{path='../protocol'}}\n"
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
    run(library.path(), "git", &["tag", &format!("v{version}")]);
    library
}

/// Move the library checkout onto an unreleased candidate: the tag still names
/// the released version, the working tree carries the next one.
fn bump_library_version(library_root: &Path, from: &str, to: &str) {
    for member in ["crates/core", "crates/protocol"] {
        let manifest = library_root.join(member).join("Cargo.toml");
        let raw = fs::read_to_string(&manifest).unwrap();
        let bumped = raw.replace(&format!("version='{from}'"), &format!("version='{to}'"));
        assert_ne!(raw, bumped, "library manifest did not declare v{from}");
        write(&manifest, &bumped);
    }
    run(library_root, "git", &["commit", "-qam", "candidate bump"]);
}

fn git_dependency(git_url: &str, version: &str) -> String {
    format!("{{git='{git_url}',tag='v{version}'}}")
}

/// A Git pin that also constrains the version, so a later candidate can never
/// satisfy it.
fn exact_git_dependency(git_url: &str, version: &str) -> String {
    format!("{{git='{git_url}',tag='v{version}',version='={version}'}}")
}

fn consumer_manifest(name: &str, dependency: &str) -> String {
    format!(
        "[package]\nname='{name}'\nversion='0.1.0'\nedition='2021'\n[dependencies]\neffigy-link-fixture-core={dependency}\n"
    )
}

fn prepare_consumer(repo: &Path, roots: &[PathBuf], dependency: &str) {
    for (index, root) in roots.iter().enumerate() {
        write(
            &root.join("Cargo.toml"),
            &consumer_manifest(&format!("effigy-link-consumer-{index}"), dependency),
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
    prepare_consumer(&repo, &roots, &git_dependency(&git_url, "0.1.0"));

    let report = execute_cargo_link(&repo, &library_root, false, &StdReadOnlyProcess).unwrap();

    assert_eq!(report.outcome, CargoLinkOutcome::Applied);
    assert_eq!(report.verification.status, VerificationStatus::Passed);
    assert!(
        report.plan.version_transitions.is_empty(),
        "a same-version link must not refresh anything: {:#?}",
        report.plan.version_transitions
    );
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
fn compatible_hand_managed_patch_is_adopted_without_dry_run_lock_churn() {
    let library = create_library();
    let library_root = fs::canonicalize(library.path()).unwrap();
    let git_url = format!("file://{}", library_root.display());
    let repo_temp = TempDir::new().unwrap();
    let repo = fs::canonicalize(repo_temp.path()).unwrap();
    prepare_consumer(
        &repo,
        std::slice::from_ref(&repo),
        &git_dependency(&git_url, "0.1.0"),
    );
    write(&repo.join(".gitignore"), ".cargo/config.toml\n.effigy/\n");
    run(&repo, "git", &["add", ".gitignore"]);
    run(&repo, "git", &["commit", "-qm", "ignore local link state"]);

    let manual_config = format!(
        "# existing local Signal patch\n[net]\ngit-fetch-with-cli = true\n\n[patch.\"{git_url}\"]\neffigy-link-fixture-core = {{ path = \"{}\" }}\neffigy-link-fixture-protocol = {{ path = \"{}\" }}\n",
        library_root.join("crates/core").display(),
        library_root.join("crates/protocol").display()
    );
    write(&repo.join(".cargo/config.toml"), &manual_config);
    let lock_before = fs::read(repo.join("Cargo.lock")).unwrap();

    let unlink_dry_run =
        execute_cargo_unlink(&repo, &library_root, true, &StdReadOnlyProcess).unwrap();
    assert_eq!(unlink_dry_run.outcome, CargoUnlinkOutcome::DryRun);
    assert!(unlink_dry_run
        .plan
        .operation
        .warnings
        .iter()
        .any(|warning| warning.contains("pre-Effigy Cargo patch")));
    assert_eq!(
        fs::read_to_string(repo.join(".cargo/config.toml")).unwrap(),
        manual_config
    );
    assert_eq!(fs::read(repo.join("Cargo.lock")).unwrap(), lock_before);

    let direct_unlink =
        execute_cargo_unlink(&repo, &library_root, false, &StdReadOnlyProcess).unwrap();
    assert_eq!(
        direct_unlink.outcome,
        CargoUnlinkOutcome::Unlinked,
        "{direct_unlink:#?}"
    );
    assert_eq!(
        direct_unlink.verification.status,
        VerificationStatus::Passed
    );
    assert!(!fs::read_to_string(repo.join(".cargo/config.toml"))
        .unwrap()
        .contains("[patch."));
    assert_eq!(fs::read(repo.join("Cargo.lock")).unwrap(), lock_before);
    write(&repo.join(".cargo/config.toml"), &manual_config);

    let dry_run = execute_cargo_link(&repo, &library_root, true, &StdReadOnlyProcess).unwrap();

    assert_eq!(dry_run.outcome, CargoLinkOutcome::DryRun);
    assert!(dry_run
        .plan
        .operation
        .warnings
        .iter()
        .any(|warning| warning.contains("adopting 1 compatible hand-managed")));
    assert_eq!(
        fs::read_to_string(repo.join(".cargo/config.toml")).unwrap(),
        manual_config
    );
    assert_eq!(fs::read(repo.join("Cargo.lock")).unwrap(), lock_before);
    assert!(!RepoLinkStateStore::for_repo(&repo).path().exists());

    let link = execute_cargo_link(&repo, &library_root, false, &StdReadOnlyProcess).unwrap();
    assert_eq!(link.outcome, CargoLinkOutcome::Applied, "{link:#?}");
    assert_eq!(link.verification.status, VerificationStatus::Passed);
    let managed_config = fs::read_to_string(repo.join(".cargo/config.toml")).unwrap();
    assert!(managed_config.contains("# existing local Signal patch"));
    assert!(managed_config.contains("[net]"));
    assert_eq!(managed_config.matches("# >>> effigy deps cargo").count(), 1);

    let unlink = execute_cargo_unlink(&repo, &library_root, false, &StdReadOnlyProcess).unwrap();
    assert_eq!(unlink.outcome, CargoUnlinkOutcome::Unlinked, "{unlink:#?}");
    assert_eq!(unlink.verification.status, VerificationStatus::Passed);
    let unlinked_config = fs::read_to_string(repo.join(".cargo/config.toml")).unwrap();
    assert!(unlinked_config.contains("# existing local Signal patch"));
    assert!(unlinked_config.contains("[net]"));
    assert!(!unlinked_config.contains("[patch."));
    assert!(!unlinked_config.contains("effigy deps cargo"));
    assert_eq!(fs::read(repo.join("Cargo.lock")).unwrap(), lock_before);
    assert!(!RepoLinkStateStore::for_repo(&repo).path().exists());
}

#[test]
fn unlink_preserves_foreign_cargo_state_and_another_active_library() {
    let first = create_library();
    let second = create_named_library(
        "effigy-link-other-core",
        "effigy-link-other-protocol",
        "0.1.0",
    );
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

/// The released tag and the local candidate carry different versions, so the
/// consumer lockfile pins a version the patch cannot supply.
fn create_release_library(version: &str) -> TempDir {
    create_named_library(
        "effigy-link-fixture-core",
        "effigy-link-fixture-protocol",
        version,
    )
}

#[test]
fn real_version_transition_links_the_local_candidate_over_the_pinned_release() {
    let library = create_release_library("0.4.3");
    let library_root = fs::canonicalize(library.path()).unwrap();
    let git_url = format!("file://{}", library_root.display());
    let repo_temp = TempDir::new().unwrap();
    let repo = fs::canonicalize(repo_temp.path()).unwrap();
    prepare_consumer(
        &repo,
        std::slice::from_ref(&repo),
        &git_dependency(&git_url, "0.4.3"),
    );
    let baseline = fs::read_to_string(repo.join("Cargo.lock")).unwrap();
    assert!(baseline.contains("version = \"0.4.3\""), "{baseline}");
    bump_library_version(&library_root, "0.4.3", "0.4.4");

    let report = execute_cargo_link(&repo, &library_root, false, &StdReadOnlyProcess).unwrap();

    assert_eq!(report.outcome, CargoLinkOutcome::Applied, "{report:#?}");
    assert_eq!(report.verification.status, VerificationStatus::Passed);
    assert_eq!(
        report.plan.version_transitions,
        [
            CargoVersionTransition {
                consumer_root: repo.clone(),
                package: "effigy-link-fixture-core".to_owned(),
                locked_version: "0.4.3".to_owned(),
                local_version: "0.4.4".to_owned(),
            },
            CargoVersionTransition {
                consumer_root: repo.clone(),
                package: "effigy-link-fixture-protocol".to_owned(),
                locked_version: "0.4.3".to_owned(),
                local_version: "0.4.4".to_owned(),
            },
        ]
    );
    let linked = fs::read_to_string(repo.join("Cargo.lock")).unwrap();
    assert!(!linked.contains("[[patch.unused]]"), "{linked}");
    assert!(linked.contains("version = \"0.4.4\""), "{linked}");
    assert!(!linked.contains("tag=v0.4.3"), "{linked}");
    run(
        &repo,
        "cargo",
        &[
            "check",
            "--manifest-path",
            repo.join("Cargo.toml").to_str().unwrap(),
        ],
    );

    let unlink = execute_cargo_unlink(&repo, &library_root, false, &StdReadOnlyProcess).unwrap();

    assert_eq!(unlink.outcome, CargoUnlinkOutcome::Unlinked, "{unlink:#?}");
    assert_eq!(unlink.verification.status, VerificationStatus::Passed);
    assert!(unlink
        .lockfiles
        .iter()
        .all(|lock| lock.after_state == CargoLockfileState::Clean));
    assert_eq!(
        fs::read_to_string(repo.join("Cargo.lock")).unwrap(),
        baseline
    );
}

#[test]
fn failed_verification_restores_every_affected_lockfile_across_the_version_transition() {
    let library = create_release_library("0.4.3");
    let library_root = fs::canonicalize(library.path()).unwrap();
    let git_url = format!("file://{}", library_root.display());
    let repo_temp = TempDir::new().unwrap();
    let repo = fs::canonicalize(repo_temp.path()).unwrap();
    let roots = vec![repo.join("apps/one"), repo.join("apps/two")];
    prepare_consumer(&repo, &roots, &exact_git_dependency(&git_url, "0.4.3"));
    let baselines = roots
        .iter()
        .map(|root| fs::read_to_string(root.join("Cargo.lock")).unwrap())
        .collect::<Vec<_>>();
    bump_library_version(&library_root, "0.4.3", "0.4.4");

    let report = execute_cargo_link(&repo, &library_root, false, &StdReadOnlyProcess).unwrap();

    assert_eq!(
        report.outcome,
        CargoLinkOutcome::VerificationFailed,
        "{report:#?}"
    );
    assert!(
        report
            .errors
            .iter()
            .any(|error| error.contains("`effigy-link-fixture-core`")
                && error.contains("[[patch.unused]]")),
        "verification must name the unapplied patch: {report:#?}"
    );
    assert!(report.rollback.attempted);
    assert!(report.rollback.failures.is_empty(), "{report:#?}");
    for (root, baseline) in roots.iter().zip(&baselines) {
        let lockfile = root.join("Cargo.lock");
        assert!(report.rollback.restored.contains(&lockfile), "{report:#?}");
        assert_eq!(&fs::read_to_string(&lockfile).unwrap(), baseline);
    }
    assert!(!repo.join(".cargo/config.toml").exists());
    assert!(!repo.join(".cargo").exists());
    assert!(!RepoLinkStateStore::for_repo(&repo).path().exists());
    assert!(
        run(&repo, "git", &["status", "--porcelain=v1"]).is_empty(),
        "rolled-back link left the consumer checkout dirty"
    );
}
