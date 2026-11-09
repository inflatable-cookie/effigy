//! Changelog fragments through the real release Run (contract 054 §3, §5).
//!
//! These use the same fake-forge fixture as the rest of this module: a whole
//! project with its own bare remote and HOME, and `TestEffects` in place of
//! GitHub, tags and publication. Nothing reaches a real remote.

use super::*;

const FRAGMENT_CHANGELOG: &str = "# Changelog\n\n## [Unreleased]\n\n## [0.2.4] - 2026-10-01\n\n### Fixed\n- A previous release.\n";

/// The single-identity project at 0.2.4, with its changelog moved to
/// fragments: an empty `[Unreleased]`, `.gitkeep`, and three fragments.
fn fragment_project(fixture: &Fixture) {
    single_identity_project(fixture);
    write(&fixture.root, "CHANGELOG.md", FRAGMENT_CHANGELOG);
    write(&fixture.root, "changelog.d/.gitkeep", "");
    write(
        &fixture.root,
        "changelog.d/804-fixed-one.fixed.md",
        "Fix one.\n",
    );
    write(
        &fixture.root,
        "changelog.d/805-added-two.added.md",
        "Add two.\n",
    );
    write(
        &fixture.root,
        "changelog.d/806-security-three.security.md",
        "Harden three.\n",
    );
    commit_all(&fixture.root, "fragments");
}

fn fragment_fixture(label: &str) -> Fixture {
    let fixture = fixture(label);
    fragment_project(&fixture);
    fixture
}

/// Every byte the fragment project owns, keyed by repository-relative path.
fn fragment_bytes(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    for relative in [
        "CHANGELOG.md",
        "Cargo.toml",
        "changelog.d/.gitkeep",
        "changelog.d/804-fixed-one.fixed.md",
        "changelog.d/805-added-two.added.md",
        "changelog.d/806-security-three.security.md",
    ] {
        if let Ok(bytes) = std::fs::read(root.join(relative)) {
            files.insert(relative.to_owned(), bytes);
        }
    }
    files
}

#[test]
fn a_fragment_prepare_folds_exactly_the_planned_set_and_leaves_unreleased_empty() {
    let fixture = fragment_fixture("frag-fold");
    let mut effects = TestEffects::new(&fixture.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());

    let outcome = run(&fixture, &mut effects, ReleaseAction::Run, None);

    assert!(
        outcome.succeeded(),
        "run failed: {:?}",
        outcome.record.errors
    );
    assert_eq!(outcome.record.status, ReleaseRunStatus::Complete);
    let changelog =
        std::fs::read_to_string(fixture.root.join("CHANGELOG.md")).expect("CHANGELOG.md");
    assert!(
        changelog.starts_with("# Changelog\n"),
        "published preamble kept: {changelog}"
    );
    assert!(
        changelog.contains("## [Unreleased]\n\n## [0.2.5] - "),
        "Unreleased is empty and the new version follows it: {changelog}"
    );
    assert!(
        changelog.contains(
            "### Added\n- Add two.\n\n### Fixed\n- Fix one.\n\n### Security\n- Harden three.\n\n## [0.2.4] - 2026-10-01"
        ),
        "fragments fold in profile category order with the published release kept below: {changelog}"
    );
    for consumed in [
        "changelog.d/804-fixed-one.fixed.md",
        "changelog.d/805-added-two.added.md",
        "changelog.d/806-security-three.security.md",
    ] {
        assert!(
            !fixture.root.join(consumed).exists(),
            "{consumed} must be consumed"
        );
    }
    assert!(
        fixture.root.join("changelog.d/.gitkeep").is_file(),
        "the adoption marker survives"
    );
    assert_eq!(git(&fixture.root, &["status", "--porcelain"]), "");
    let folded = git(
        &fixture.root,
        &["show", "--name-status", "--format=", "HEAD"],
    );
    assert!(
        folded.contains("D\tchangelog.d/804-fixed-one.fixed.md"),
        "the commit records each removal: {folded}"
    );
}

#[test]
fn a_failed_prepare_commit_restores_the_changelog_and_every_fragment_byte_for_byte() {
    let fixture = fragment_fixture("frag-commit");
    let before = fragment_bytes(&fixture.root);
    let mut effects = TestEffects::new(&fixture.root);
    effects.fail_commit = true;

    let outcome = run(&fixture, &mut effects, ReleaseAction::Run, None);

    assert_eq!(outcome.record.status, ReleaseRunStatus::Failed);
    assert!(
        outcome
            .record
            .errors
            .iter()
            .any(|error| error.contains("could not commit")),
        "{:?}",
        outcome.record.errors
    );
    assert_eq!(fragment_bytes(&fixture.root), before);
    assert_eq!(git(&fixture.root, &["status", "--porcelain"]), "");
    assert!(remote_tags(&fixture).is_empty());
}

#[test]
fn a_fragment_that_cannot_be_removed_rolls_back_the_whole_fold() {
    let fixture = fragment_fixture("frag-remove");
    let before = fragment_bytes(&fixture.root);
    // A read-only folder refuses the fragment removal after the version file
    // and the changelog were already written, so the rollback has to restore
    // them. A privileged account can still delete, so the proof is then skipped
    // rather than faked.
    let folder = fixture.root.join("changelog.d");
    // Tracked, so the tree is clean when the Run starts.
    let probe = folder.join("probe.txt");
    std::fs::write(&probe, "probe\n").expect("probe");
    commit_all(&fixture.root, "probe");
    set_mode(&folder, 0o555);
    let enforced = std::fs::remove_file(&probe).is_err();
    set_mode(&folder, 0o755);
    if !enforced {
        let _ = std::fs::remove_file(&probe);
        return;
    }
    set_mode(&folder, 0o555);

    let mut effects = TestEffects::new(&fixture.root);
    let outcome = run(&fixture, &mut effects, ReleaseAction::Run, None);

    set_mode(&folder, 0o755);
    assert_eq!(outcome.record.status, ReleaseRunStatus::Failed);
    assert!(
        outcome
            .record
            .errors
            .iter()
            .any(|error| error.contains("rolled back")),
        "{:?}",
        outcome.record.errors
    );
    assert_eq!(outcome.record.candidate_commit, None);
    assert_eq!(fragment_bytes(&fixture.root), before);
    assert_eq!(git(&fixture.root, &["status", "--porcelain"]), "");
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("mode");
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) {}

#[test]
fn a_pre_gate_failure_touches_neither_the_changelog_nor_any_fragment() {
    let fixture = fragment_fixture("frag-gate");
    write(&fixture.root, "gates/pre.sh", "#!/bin/sh\nexit 1\n");
    commit_all(&fixture.root, "failing pre gate");
    let before = fragment_bytes(&fixture.root);
    let mut effects = TestEffects::new(&fixture.root);

    let outcome = run(&fixture, &mut effects, ReleaseAction::Run, None);

    assert_eq!(outcome.record.status, ReleaseRunStatus::Failed);
    assert_eq!(outcome.record.candidate_commit, None);
    assert_eq!(fragment_bytes(&fixture.root), before);
    assert_eq!(git(&fixture.root, &["status", "--porcelain"]), "");
}

#[test]
fn a_malformed_fragment_stops_the_run_before_any_mutation() {
    let fixture = fragment_fixture("frag-malformed");
    write(
        &fixture.root,
        "changelog.d/807-bad.deprecated.md",
        "Not a canonical type.\n",
    );
    commit_all(&fixture.root, "malformed fragment");
    let before = fragment_bytes(&fixture.root);
    let mut effects = TestEffects::new(&fixture.root);

    let outcome = run(&fixture, &mut effects, ReleaseAction::Run, None);

    assert_ne!(outcome.record.status, ReleaseRunStatus::Complete);
    assert_eq!(outcome.record.candidate_commit, None);
    assert!(
        outcome
            .record
            .errors
            .iter()
            .any(|error| error.contains("807-bad.deprecated.md")),
        "the error names the malformed file: {:?}",
        outcome.record.errors
    );
    assert_eq!(fragment_bytes(&fixture.root), before);
}

#[test]
fn a_fragment_that_changes_after_the_plan_is_refused_before_a_partial_fold() {
    // Planning-boundary proof: the planned set is read, the bytes change, and
    // prepare's mutation planning refuses before any file is written. This is
    // not an end-to-end prepare; the Run tests above are.
    let fixture = fragment_fixture("frag-drift");
    let plan = crate::plan_release_pipeline(&fixture.root).expect("release plan");
    assert_eq!(plan.changelog_fragments.len(), 3);
    write(
        &fixture.root,
        "changelog.d/805-added-two.added.md",
        "Edited after the plan.\n",
    );
    let before = fragment_bytes(&fixture.root);

    let error = plan_prepare_mutations(&plan).expect_err("drift must refuse");

    assert!(
        error
            .to_string()
            .contains("changed since the release was planned"),
        "{error}"
    );
    assert_eq!(fragment_bytes(&fixture.root), before);
}

#[test]
fn a_new_fragment_after_the_plan_is_refused_too() {
    let fixture = fragment_fixture("frag-new");
    let plan = crate::plan_release_pipeline(&fixture.root).expect("release plan");
    write(&fixture.root, "changelog.d/807-late.fixed.md", "Late.\n");

    let error = plan_prepare_mutations(&plan).expect_err("a new fragment must refuse");

    assert!(
        error
            .to_string()
            .contains("changed since the release was planned"),
        "{error}"
    );
}
