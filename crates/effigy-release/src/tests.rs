use super::prepare_helpers::unexpected_lockfile_change;
use super::{
    build_release_prepare_plan, compare_release_state_fingerprints, format_release_tag,
    gate_blockers, git_create_tag, is_release_state_file, load_release_config,
    load_release_context, load_release_prepared_state, normalized_expected_files,
    restore_mutation_snapshots, snapshot_mutation_paths, test_support,
    validate_planned_release_version, write_release_prepared_state, FileMutationApply,
    FileMutationPlan, GateExecutionReport, GateResult, ReleasePreparedFileFingerprint,
    ReleasePreparedSourceFingerprints,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn temp_repo(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "effigy-release-{name}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("mkdir");
    root
}

fn write_initial_release_repo(name: &str, opt_in: bool) -> PathBuf {
    let root = temp_repo(name);
    let initial_mode = if opt_in {
        "initial-tag-current-version = true\n"
    } else {
        ""
    };
    fs::write(
        root.join("effigy.toml"),
        format!(
            "[release]\nversion-file = \"VERSION\"\nchangelog = \"CHANGELOG.md\"\n{initial_mode}"
        ),
    )
    .expect("manifest");
    fs::write(root.join("VERSION"), "0.1.0\n").expect("version");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\n## [Unreleased]\n\n### Added\n- Initial release\n",
    )
    .expect("changelog");
    let status = Command::new("git")
        .arg("init")
        .arg("--quiet")
        .arg(&root)
        .status()
        .expect("git init");
    assert!(status.success());
    root
}

#[test]
fn release_tag_is_annotated_with_the_rendered_tag_as_its_message() {
    let root = temp_repo("annotated-tag");
    fs::write(root.join("README.md"), "release fixture\n").expect("fixture");
    for args in [
        &["init", "--quiet"][..],
        &["config", "user.name", "Effigy Test"][..],
        &["config", "user.email", "effigy@example.invalid"][..],
        &["add", "README.md"][..],
        &["commit", "--quiet", "-m", "initial"][..],
    ] {
        let status = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(args)
            .status()
            .expect("git fixture command");
        assert!(status.success(), "git command failed: {args:?}");
    }

    let head = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("head");
    let head = String::from_utf8(head.stdout).expect("head utf8");

    git_create_tag(&root, "v0.1.0").expect("annotated tag");

    let object_type = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["cat-file", "-t", "refs/tags/v0.1.0"])
        .output()
        .expect("tag type");
    assert_eq!(
        String::from_utf8(object_type.stdout)
            .expect("tag type utf8")
            .trim(),
        "tag"
    );

    let message = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["for-each-ref", "--format=%(contents)", "refs/tags/v0.1.0"])
        .output()
        .expect("tag message");
    assert_eq!(
        String::from_utf8(message.stdout)
            .expect("tag message utf8")
            .trim(),
        "v0.1.0"
    );

    let peeled = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["rev-parse", "refs/tags/v0.1.0^{}"])
        .output()
        .expect("peeled tag");
    assert_eq!(
        String::from_utf8(peeled.stdout)
            .expect("peeled utf8")
            .trim(),
        head.trim()
    );
}

#[test]
fn version_file_kind_detection_matches_supported_names() {
    test_support::assert_supported_version_file_kinds();
}

#[test]
fn version_field_path_defaults_follow_known_formats() {
    test_support::assert_default_version_field_paths();
}

#[test]
fn detect_cargo_version_path_supports_workspace_inherited_versions() {
    test_support::assert_workspace_inherited_cargo_version_path();
}

#[test]
fn load_release_config_reads_manifest_release_settings() {
    let root = temp_repo("config");
    fs::write(
        root.join("effigy.toml"),
        r#"
[release]
version-file = "VERSION"
changelog = "docs/CHANGELOG.md"
pre-1-0 = false
tag-format = "release-{version}"
sync-files = ["Cargo.lock"]

[release.gates.qa]
command = "cargo test"
description = "Run tests"
"#,
    )
    .expect("write manifest");
    fs::write(root.join("VERSION"), "0.2.4\n").expect("version");
    fs::create_dir_all(root.join("docs")).expect("docs dir");
    fs::write(root.join("docs/CHANGELOG.md"), "# Changelog\n").expect("changelog");

    let error = load_release_config(&root).expect_err("Cargo.lock should be rejected for VERSION");
    assert!(error
        .to_string()
        .contains("`Cargo.lock` is only supported when the release version file is Cargo.toml"));
}

#[test]
fn initial_tag_current_version_selects_current_and_omits_version_mutation() {
    let root = write_initial_release_repo("initial-current", true);
    let context = load_release_context(&root).expect("release context");

    assert_eq!(context.next_version, Some(semver::Version::new(0, 1, 0)));
    assert_eq!(context.suggested_bump, super::BumpKind::None);
    assert_eq!(context.tag.as_deref(), Some("v0.1.0"));
    assert!(context.blockers.is_empty(), "{:?}", context.blockers);

    let plan = build_release_prepare_plan(&context, false, GateExecutionReport::empty(), None)
        .expect("prepare plan");
    assert!(plan.ready, "{:?}", plan.blockers);
    assert_eq!(
        plan.mutations
            .iter()
            .map(|mutation| mutation.kind)
            .collect::<Vec<_>>(),
        vec!["changelog"]
    );
}

#[test]
fn current_version_release_requires_the_first_tag_opt_in() {
    let root = write_initial_release_repo("initial-disabled", false);
    let context = load_release_context(&root).expect("release context");
    let current = semver::Version::new(0, 1, 0);
    let lower = semver::Version::new(0, 0, 9);

    assert!(validate_planned_release_version(&context, &current)
        .expect_err("current version should be rejected")
        .contains("must be greater than current version"));
    assert!(validate_planned_release_version(&context, &lower)
        .expect_err("lower version should be rejected")
        .contains("must be greater than current version"));
}

#[test]
fn initial_tag_current_version_rejects_an_existing_local_tag() {
    let root = write_initial_release_repo("initial-existing-tag", true);
    let object = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["hash-object", "-w", "VERSION"])
        .output()
        .expect("hash version file");
    assert!(object.status.success());
    let object_id = String::from_utf8(object.stdout).expect("object id");
    let status = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["update-ref", "refs/tags/v0.1.0", object_id.trim()])
        .status()
        .expect("write tag ref");
    assert!(status.success());

    let context = load_release_context(&root).expect("release context");
    assert!(context
        .blockers
        .iter()
        .any(|blocker| blocker == "initial release tag already exists locally: v0.1.0"));
}

#[test]
fn initial_tag_current_version_closes_after_the_first_changelog_release() {
    let root = write_initial_release_repo("initial-after-release", true);
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\n## [Unreleased]\n\n### Fixed\n- Follow-up\n\n## [0.1.0] - 2026-08-05\n\n### Added\n- Initial release\n",
    )
    .expect("changelog");

    let context = load_release_context(&root).expect("release context");
    assert_eq!(context.next_version, Some(semver::Version::new(0, 1, 1)));
    assert!(
        validate_planned_release_version(&context, &semver::Version::new(0, 1, 0))
            .expect_err("released current version should be rejected")
            .contains("must be greater than current version")
    );
}

#[test]
fn gate_helpers_return_expected_defaults() {
    let blockers = gate_blockers(&[GateResult {
        name: "qa".to_owned(),
        description: None,
        command: "cargo test".to_owned(),
        passed: false,
        exit_code: Some(1),
        stdout: String::new(),
        stderr: String::new(),
        launch_error: None,
        duration_ms: 12,
    }]);
    assert_eq!(blockers, vec!["gate `qa` failed".to_owned()]);
    assert_eq!(GateExecutionReport::empty().results.len(), 0);
    assert_eq!(
        format_release_tag("release-{version}", &semver::Version::new(0, 2, 5)),
        "release-0.2.5"
    );
}

#[test]
fn prepared_state_round_trip_preserves_fingerprints() {
    let root = temp_repo("prepared-state");
    let version_file = root.join("VERSION");
    fs::write(&version_file, "0.3.0\n").expect("version");
    let state_path = root.join(".release-prepared.json");

    write_release_prepared_state(crate::prepare_helpers::ReleasePreparedStateWrite {
        path: &state_path,
        repo_root: &root,
        previous_version: &semver::Version::parse("0.2.9").expect("previous"),
        suggested_version: Some(&semver::Version::parse("0.3.0").expect("suggested")),
        prepared_version: Some(&semver::Version::parse("0.3.0").expect("prepared")),
        suggested_tag: Some("v0.3.0"),
        tag: Some("v0.3.0"),
        version_override_used: false,
        release_date: "2026-04-16",
        gates_checked: true,
        files_modified: std::slice::from_ref(&version_file),
        prepared_branch: Some("main"),
        prepared_head: Some("deadbeef"),
    })
    .expect("write state");

    let state = load_release_prepared_state(&state_path).expect("load state");
    assert_eq!(state.prepared_version.to_string(), "0.3.0");
    assert_eq!(
        state
            .source_fingerprints
            .as_ref()
            .and_then(|value| value.prepared_branch.as_deref()),
        Some("main")
    );
    assert_eq!(
        state
            .source_fingerprints
            .as_ref()
            .and_then(|value| value.prepared_head.as_deref()),
        Some("deadbeef")
    );
    assert_eq!(
        state
            .source_fingerprints
            .as_ref()
            .map(|value| value.files.len()),
        Some(1)
    );
}

/// The prepared-state file must NOT be required as a working-tree change.
///
/// It used to be, and the presence check runs through `git status`, which
/// honours `.gitignore` -- so in any repository that gitignores the file (both
/// signal and swallowtail do) execute reported it permanently missing and could
/// never run at all.
#[test]
fn normalized_expected_files_excludes_the_state_file() {
    let repo_root = PathBuf::from("/tmp/repo");
    let files = vec![
        repo_root.join("Cargo.toml"),
        repo_root.join("CHANGELOG.md"),
        repo_root.join("Cargo.toml"),
    ];
    let normalized = normalized_expected_files(".release-prepared.json", &repo_root, &files);
    assert_eq!(
        normalized,
        vec!["CHANGELOG.md".to_owned(), "Cargo.toml".to_owned()]
    );
    // Tolerated, not required: a repository that does track it must not have it
    // counted as an unexpected change either.
    assert!(is_release_state_file(
        ".release-prepared.json",
        ".release-prepared.json"
    ));
    assert!(!is_release_state_file(
        "Cargo.toml",
        ".release-prepared.json"
    ));
}

/// A failed prepare must not leave the version bump on disk.
#[test]
fn restore_mutation_snapshots_puts_the_tree_back() {
    let root = temp_repo("rollback");
    let existing = root.join("Cargo.toml");
    let created = root.join("CHANGELOG.md");
    fs::write(&existing, "version = \"0.1.0\"\n").expect("seed");

    let mut snapshots = BTreeMap::new();
    snapshots.insert(existing.clone(), Some(b"version = \"0.1.0\"\n".to_vec()));
    snapshots.insert(created.clone(), None);

    // Simulate the mutations prepare would have applied before a gate failed.
    fs::write(&existing, "version = \"0.2.0\"\n").expect("mutate");
    fs::write(&created, "# Changelog\n").expect("create");

    let unrestored = restore_mutation_snapshots(&snapshots);

    assert!(unrestored.is_empty(), "nothing should fail to restore");
    assert_eq!(
        fs::read_to_string(&existing).expect("read"),
        "version = \"0.1.0\"\n"
    );
    // Absent before, so removed rather than left as an empty stub.
    assert!(!created.exists());
}

/// The lockfile sync must move workspace versions and nothing else.
#[test]
fn unexpected_lockfile_change_distinguishes_workspace_from_third_party() {
    let before = "[[package]]\nname = \"signal-dsp\"\nversion = \"0.1.0\"\n\n\
                  [[package]]\nname = \"rayon\"\nversion = \"1.11.0\"\n";

    // The whole point of the sync: workspace members move to the bumped version.
    let workspace_bump = "[[package]]\nname = \"signal-dsp\"\nversion = \"0.1.1\"\n\n\
                          [[package]]\nname = \"rayon\"\nversion = \"1.11.0\"\n";
    assert_eq!(
        unexpected_lockfile_change(before, workspace_bump, "0.1.1"),
        None
    );

    // What `cargo generate-lockfile` used to do, and what must now be refused:
    // a third-party crate moving. It is also a `version` line, which is why
    // checking the added value rather than the line shape is what catches it.
    let third_party = "[[package]]\nname = \"signal-dsp\"\nversion = \"0.1.1\"\n\n\
                       [[package]]\nname = \"rayon\"\nversion = \"1.12.0\"\n";
    let refused = unexpected_lockfile_change(before, third_party, "0.1.1")
        .expect("a third-party bump must be refused");
    assert!(refused.contains("1.12.0"), "{refused}");

    // Anything structural -- a package appearing or disappearing -- is refused
    // outright rather than inspected.
    let added_package = "[[package]]\nname = \"signal-dsp\"\nversion = \"0.1.1\"\n\n\
                         [[package]]\nname = \"rayon\"\nversion = \"1.11.0\"\n\n\
                         [[package]]\nname = \"surprise\"\nversion = \"0.1.1\"\n";
    let refused = unexpected_lockfile_change(before, added_package, "0.1.1")
        .expect("a new package must be refused");
    assert!(refused.contains("surprise"), "{refused}");

    // No change at all is fine.
    assert_eq!(unexpected_lockfile_change(before, before, "0.1.0"), None);
}

#[test]
fn compare_release_state_fingerprints_reports_branch_head_and_file_drift() {
    let root = temp_repo("fingerprint-drift");
    let file = root.join("VERSION");
    fs::write(&file, "0.3.0\n").expect("version");
    let drift = compare_release_state_fingerprints(
        &root,
        &ReleasePreparedSourceFingerprints {
            prepared_branch: Some("main".to_owned()),
            prepared_head: Some("abc".to_owned()),
            files: vec![ReleasePreparedFileFingerprint {
                path: PathBuf::from("VERSION"),
                digest: "wrong".to_owned(),
            }],
        },
        Some("feature"),
        Some("def"),
    );
    assert_eq!(drift.len(), 3);
}

#[test]
fn snapshot_mutation_paths_reads_unique_paths() {
    let root = temp_repo("snapshots");
    let file = root.join("VERSION");
    fs::write(&file, "0.2.9\n").expect("version");
    let plan = vec![
        FileMutationPlan {
            path: file.clone(),
            kind: "version-file",
            summary: "test".to_owned(),
            before_preview: String::new(),
            after_preview: String::new(),
            detail_lines: Vec::new(),
            diff_preview: Vec::new(),
            apply: FileMutationApply::Write {
                after_contents: "0.3.0\n".to_owned(),
            },
        },
        FileMutationPlan {
            path: file.clone(),
            kind: "version-file",
            summary: "duplicate".to_owned(),
            before_preview: String::new(),
            after_preview: String::new(),
            detail_lines: Vec::new(),
            diff_preview: Vec::new(),
            apply: FileMutationApply::Write {
                after_contents: "0.3.1\n".to_owned(),
            },
        },
    ];

    let snapshots = snapshot_mutation_paths(&plan).expect("snapshots");
    assert_eq!(snapshots.len(), 1);
    assert_eq!(
        snapshots
            .get(&file)
            .and_then(|value| value.as_ref())
            .cloned(),
        Some(b"0.2.9\n".to_vec())
    );
}
