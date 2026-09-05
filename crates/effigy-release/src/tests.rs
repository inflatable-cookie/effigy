use super::prepare_helpers::unexpected_lockfile_change;
use super::{
    apply_release_mutations, build_release_prepare_plan, collect_release_gate_run,
    compare_release_state_fingerprints, execute_release_prepare, format_release_tag, gate_blockers,
    git_create_tag, git_modified_files, is_release_state_file, load_release_config,
    load_release_context, load_release_prepared_state, normalized_expected_files,
    render_release_gate_run_json, render_release_gate_run_text, render_release_prepare_plan_text,
    render_release_prepared_text, restore_mutation_snapshots, run_release_gates,
    snapshot_mutation_paths, test_support, validate_planned_release_version,
    write_release_prepared_state, FileMutationApply, FileMutationPlan, GateExecutionReport,
    GateResult, ReleasePreparedFileFingerprint, ReleasePreparedSourceFingerprints, ResolvedGate,
    ResolvedVersionSource, VersionFileKind,
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
fn prepare_plan_coordinates_workspace_path_dependency_versions() {
    let root = temp_repo("coordinated-workspace-versions");
    let external = temp_repo("external-workspace-dependency");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nversion-file = \"Cargo.toml\"\nversion-path = \
         \"workspace.package.version\"\nchangelog = \"CHANGELOG.md\"\nsync-files = \
         [\"Cargo.lock\"]\n",
    )
    .expect("manifest");
    let cargo_before = format!(
        r#"[workspace]
members = ["crates/*"]
exclude = ["crates/excluded"]
resolver = "2"

[workspace.package]
version = "0.3.1"
edition = "2021"

[workspace.dependencies]
fixture-core = {{ path = "crates/core", version = "0.3.1" }}
fixture-renamed = {{ package = "fixture-alias", path = "crates/alias", version = "0.3.1" }}
fixture-independent = {{ path = "crates/independent", version = "0.3.1" }}
fixture-excluded = {{ path = "crates/excluded", version = "0.3.1" }}
fixture-external = {{ path = "{}", version = "0.3.1" }}
fixture-git = {{ git = "https://example.invalid/fixture.git", version = "0.3.1" }}
serde = "1"
"#,
        external.display()
    );
    fs::write(root.join("Cargo.toml"), &cargo_before).expect("cargo manifest");
    for (path, name, inherited) in [
        ("crates/core", "fixture-core", true),
        ("crates/alias", "fixture-alias", true),
        ("crates/independent", "fixture-independent", false),
        ("crates/excluded", "fixture-excluded", true),
    ] {
        fs::create_dir_all(root.join(path)).expect("member dir");
        let version = if inherited {
            "version.workspace = true"
        } else {
            "version = \"0.3.1\""
        };
        fs::write(
            root.join(path).join("Cargo.toml"),
            format!("[package]\nname = \"{name}\"\n{version}\nedition = \"2021\"\n"),
        )
        .expect("member manifest");
    }
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\n## [Unreleased]\n\n### Fixed\n- Coordinate workspace versions\n",
    )
    .expect("changelog");
    fs::write(
        root.join("Cargo.lock"),
        "version = 4\n\n[[package]]\nname = \"fixture-core\"\nversion = \"0.3.1\"\n",
    )
    .expect("lockfile");
    let status = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(&root)
        .status()
        .expect("git init");
    assert!(status.success());

    let context = load_release_context(&root).expect("release context");
    let plan = build_release_prepare_plan(&context, false, GateExecutionReport::empty(), None)
        .expect("prepare plan");
    assert!(plan.ready, "{:?}", plan.blockers);
    assert_eq!(plan.planned_version, Some(semver::Version::new(0, 3, 2)));

    let version_mutation = plan
        .mutations
        .iter()
        .find(|mutation| mutation.kind == "version-file")
        .expect("version mutation");
    let FileMutationApply::Write { after_contents } = &version_mutation.apply else {
        panic!("version mutation must write Cargo.toml")
    };
    assert!(after_contents.contains("version = \"0.3.2\"\n"));
    assert!(
        after_contents.contains("fixture-core = { path = \"crates/core\", version = \"0.3.2\" }")
    );
    assert!(after_contents.contains(
        "fixture-renamed = { package = \"fixture-alias\", path = \"crates/alias\", version = \
         \"0.3.2\" }"
    ));
    assert!(after_contents
        .contains("fixture-independent = { path = \"crates/independent\", version = \"0.3.1\" }"));
    assert!(after_contents
        .contains("fixture-excluded = { path = \"crates/excluded\", version = \"0.3.1\" }"));
    assert!(after_contents.contains("fixture-external"));
    assert!(after_contents.contains("version = \"0.3.1\" }\nfixture-git"));
    assert!(after_contents.contains(
        "fixture-git = { git = \"https://example.invalid/fixture.git\", version = \"0.3.1\" }"
    ));
    assert!(version_mutation
        .diff_preview
        .iter()
        .any(|line| line.contains("fixture-core") && line.contains("0.3.2")));
    assert!(version_mutation
        .detail_lines
        .contains(&"coordinated workspace dependency: fixture-core -> 0.3.2".to_owned()));
    assert!(version_mutation
        .detail_lines
        .contains(&"coordinated workspace dependency: fixture-renamed -> 0.3.2".to_owned()));
    let plan_text = super::render_release_prepare_plan_text(&plan);
    let plan_json = super::render_release_prepare_plan_json(&plan);
    for dependency in ["fixture-core", "fixture-renamed"] {
        assert!(plan_text.contains(dependency));
        assert!(plan_json.contains(dependency));
    }
    assert!(plan.mutations.iter().any(|mutation| matches!(
        mutation.apply,
        FileMutationApply::SyncCargoLock { ref workspace_version }
            if workspace_version == "0.3.2"
    )));

    let snapshots = snapshot_mutation_paths(&plan.mutations).expect("mutation snapshots");
    fs::write(root.join("Cargo.toml"), after_contents).expect("apply version mutation");
    assert!(restore_mutation_snapshots(&snapshots).is_empty());
    assert_eq!(
        fs::read_to_string(root.join("Cargo.toml")).expect("restored manifest"),
        cargo_before
    );
}

#[test]
fn prepare_plan_syncs_secondary_package_json_version() {
    let root = temp_repo("secondary-package-version");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nversion-file = \"Cargo.toml\"\nchangelog = \"CHANGELOG.md\"\nsync-files = [\"package.json\"]\n",
    )
    .expect("manifest");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.3.1\"\nedition = \"2021\"\n",
    )
    .expect("cargo manifest");
    fs::write(
        root.join("package.json"),
        "{\n  \"name\": \"fixture\",\n  \"version\"  :  \"0.3.1\"\n}\n",
    )
    .expect("package manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\n## [Unreleased]\n\n### Fixed\n- Sync package metadata\n",
    )
    .expect("changelog");
    let status = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(&root)
        .status()
        .expect("git init");
    assert!(status.success());

    let context = load_release_context(&root).expect("release context");
    let plan = build_release_prepare_plan(&context, false, GateExecutionReport::empty(), None)
        .expect("prepare plan");
    assert!(plan.ready, "{:?}", plan.blockers);
    let package_mutation = plan
        .mutations
        .iter()
        .find(|mutation| mutation.path == root.join("package.json"))
        .expect("package mutation");
    assert_eq!(package_mutation.kind, "sync-version-file");
    assert!(package_mutation
        .diff_preview
        .iter()
        .any(|line| line.contains("\"0.3.2\"")));

    apply_release_mutations(&root, &plan.mutations).expect("apply release mutations");
    let package_json = fs::read_to_string(root.join("package.json")).expect("read package");
    assert!(package_json.contains("\"version\"  :  \"0.3.2\""));
}

#[test]
fn load_release_config_rejects_secondary_package_json_without_version() {
    let root = temp_repo("secondary-package-missing-version");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nversion-file = \"Cargo.toml\"\nchangelog = \"CHANGELOG.md\"\nsync-files = [\"package.json\"]\n",
    )
    .expect("manifest");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.3.1\"\nedition = \"2021\"\n",
    )
    .expect("cargo manifest");
    fs::write(root.join("package.json"), "{\"name\":\"fixture\"}\n").expect("package manifest");
    fs::write(root.join("CHANGELOG.md"), "# Changelog\n").expect("changelog");

    let error = load_release_config(&root).expect_err("missing package version should fail");
    assert!(error
        .to_string()
        .contains("release version path `version` was not found"));
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
        log_path: None,
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
    let workspace_members = BTreeMap::from([("signal-dsp".to_owned(), "0.1.1".to_owned())]);
    let before = "[[package]]\nname = \"signal-dsp\"\nversion = \"0.1.0\"\n\n\
                  [[package]]\nname = \"rayon\"\nversion = \"1.11.0\"\n";

    // The whole point of the sync: workspace members move to the bumped version.
    let workspace_bump = "[[package]]\nname = \"signal-dsp\"\nversion = \"0.1.1\"\n\n\
                          [[package]]\nname = \"rayon\"\nversion = \"1.11.0\"\n";
    assert_eq!(
        unexpected_lockfile_change(before, workspace_bump, &workspace_members),
        None
    );

    // Mixed-version workspaces are valid: each source-less member must match
    // its own post-mutation metadata version, not one global release target.
    let mixed_members = BTreeMap::from([
        ("signal-dsp".to_owned(), "0.1.1".to_owned()),
        ("independent".to_owned(), "0.3.1".to_owned()),
    ]);
    let mixed_before =
        format!("{before}\n[[package]]\nname = \"independent\"\nversion = \"0.3.1\"\n");
    let mixed_after =
        format!("{workspace_bump}\n[[package]]\nname = \"independent\"\nversion = \"0.3.1\"\n");
    assert_eq!(
        unexpected_lockfile_change(&mixed_before, &mixed_after, &mixed_members),
        None
    );
    let flattened_after = mixed_after.replacen(
        "name = \"independent\"\nversion = \"0.3.1\"",
        "name = \"independent\"\nversion = \"0.1.1\"",
        1,
    );
    assert!(
        unexpected_lockfile_change(&mixed_before, &flattened_after, &mixed_members)
            .is_some_and(|reason| reason.contains("metadata version 0.3.1"))
    );

    // What `cargo generate-lockfile` used to do, and what must now be refused:
    // a third-party crate moving. Its package identity keeps the version field
    // outside the set of authorized workspace-member changes.
    let third_party = "[[package]]\nname = \"signal-dsp\"\nversion = \"0.1.1\"\n\n\
                       [[package]]\nname = \"rayon\"\nversion = \"1.12.0\"\n";
    let refused = unexpected_lockfile_change(before, third_party, &workspace_members)
        .expect("a third-party bump must be refused");
    assert!(
        refused.contains("outside actual workspace member"),
        "{refused}"
    );

    // The old value-only validator accepted this when the third party happened
    // to move to the selected workspace version. Package identity, not the
    // added version literal, must decide which lock entry may change.
    let same_as_workspace = "[[package]]\nname = \"signal-dsp\"\nversion = \"0.1.1\"\n\n\
                             [[package]]\nname = \"rayon\"\nversion = \"0.1.1\"\n";
    let refused = unexpected_lockfile_change(before, same_as_workspace, &workspace_members)
        .expect("a third-party move to the workspace version must be refused");
    assert!(
        refused.contains("outside actual workspace member"),
        "{refused}"
    );

    // Association matters too. A line multiset cannot see two third-party
    // packages exchanging version values, but neither package is authorized
    // to change.
    let swap_before = format!("{before}\n[[package]]\nname = \"other\"\nversion = \"1.12.0\"\n");
    let swapped = "[[package]]\nname = \"signal-dsp\"\nversion = \"0.1.1\"\n\n\
                   [[package]]\nname = \"rayon\"\nversion = \"1.12.0\"\n\n\
                   [[package]]\nname = \"other\"\nversion = \"1.11.0\"\n";
    assert!(unexpected_lockfile_change(&swap_before, swapped, &workspace_members).is_some());

    // Anything structural -- a package appearing or disappearing -- is refused
    // outright rather than inspected.
    let added_package = "[[package]]\nname = \"signal-dsp\"\nversion = \"0.1.1\"\n\n\
                         [[package]]\nname = \"rayon\"\nversion = \"1.11.0\"\n\n\
                         [[package]]\nname = \"surprise\"\nversion = \"0.1.1\"\n";
    let refused = unexpected_lockfile_change(before, added_package, &workspace_members)
        .expect("a new package must be refused");
    assert!(
        refused.contains("outside actual workspace member"),
        "{refused}"
    );

    // No change at all is fine.
    assert_eq!(
        unexpected_lockfile_change(
            before,
            before,
            &BTreeMap::from([("signal-dsp".to_owned(), "0.1.0".to_owned())])
        ),
        None
    );
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

fn shell_gate(name: &str, command: &str) -> ResolvedGate {
    ResolvedGate {
        name: name.to_owned(),
        command: command.to_owned(),
        description: None,
    }
}

#[test]
fn failing_and_passing_gates_persist_logs_and_redacted_environment() {
    let root = temp_repo("gate-persist");
    std::env::set_var("CARGO_REGISTRY_TOKEN", "super-secret-token");
    let fail = shell_gate(
        "floor",
        "printf 'stdout-line\\n'; printf 'stderr-line\\n' >&2; exit 3",
    );
    let pass = shell_gate("ok", "printf pass-ok");

    let failed_report = run_release_gates(&root, &[fail], true);
    assert_eq!(failed_report.results.len(), 1);
    assert!(!failed_report.results[0].passed);
    let fail_log = failed_report.results[0]
        .log_path
        .as_ref()
        .expect("failed gate log");
    let fail_log_contents = fs::read_to_string(fail_log).expect("read fail log");
    assert!(
        fail_log_contents.contains("stdout-line"),
        "{fail_log_contents}"
    );
    assert!(
        fail_log_contents.contains("stderr-line"),
        "{fail_log_contents}"
    );
    assert!(
        fail_log_contents.contains("exit_code: 3"),
        "{fail_log_contents}"
    );

    let passed_report = run_release_gates(&root, &[pass], true);
    assert!(passed_report.results[0].passed);
    let pass_log = passed_report.results[0]
        .log_path
        .as_ref()
        .expect("passed gate log");
    let pass_log_contents = fs::read_to_string(pass_log).expect("read pass log");
    assert!(pass_log_contents.contains("pass-ok"), "{pass_log_contents}");

    let environment_path = passed_report
        .environment_path
        .as_ref()
        .expect("environment.json");
    let environment = fs::read_to_string(environment_path).expect("read environment");
    assert!(
        environment.contains("\"CARGO_REGISTRY_TOKEN\": \"<redacted>\""),
        "{environment}"
    );
    assert!(!environment.contains("super-secret-token"), "{environment}");
    assert!(environment.contains("\"shell\""), "{environment}");
    assert!(environment.contains("\"cwd\""), "{environment}");

    let passed_text =
        render_release_gate_run_text(&collect_release_gate_run(root.clone(), 1, passed_report));
    assert_eq!(
        passed_text
            .lines()
            .filter(|line| line.contains("ok: pass") || line.contains("] ok: pass"))
            .count(),
        1
    );
    assert!(!passed_text.contains("stdout:"), "{passed_text}");
}

#[test]
fn gate_persist_ignores_effigy_so_execute_plan_does_not_see_artifacts() {
    let root = temp_repo("gate-ignore");
    let status = Command::new("git")
        .arg("init")
        .arg("--quiet")
        .arg(&root)
        .status()
        .expect("git init");
    assert!(status.success());
    fs::write(root.join("README.md"), "fixture\n").expect("readme");
    assert!(!root.join(".gitignore").exists());

    let pass = shell_gate("ok", "printf pass-ok");
    let report = run_release_gates(&root, &[pass], true);
    assert!(report.results[0].passed);
    assert!(report.results[0].log_path.is_some());
    assert!(report.environment_path.is_some());
    assert!(root.join(".effigy/reports/release/gates/ok.log").is_file());
    assert!(root
        .join(".effigy/reports/release/gates/environment.json")
        .is_file());
    assert!(fs::read_to_string(root.join(".gitignore"))
        .expect("gitignore")
        .lines()
        .any(|line| line.trim() == ".effigy" || line.trim() == ".effigy/"));

    let modified = git_modified_files(&root).expect("git status");
    let unexpected_effigy = modified
        .iter()
        .filter(|path| path == &".effigy" || path.starts_with(".effigy/"))
        .collect::<Vec<_>>();
    assert!(
        unexpected_effigy.is_empty(),
        "untracked .effigy artifacts would fail release execute --plan: {unexpected_effigy:?}"
    );
}

#[test]
fn prepare_text_shows_failed_gate_tail_and_log_path() {
    let mut stdout_lines = Vec::new();
    for index in 1..=21 {
        stdout_lines.push(format!("line-{index:02}"));
    }
    let gate = GateResult {
        name: "floor".to_owned(),
        description: None,
        command: "printf fail".to_owned(),
        passed: false,
        exit_code: Some(9),
        stdout: stdout_lines.join("\n"),
        stderr: String::new(),
        launch_error: None,
        duration_ms: 4,
        log_path: Some(PathBuf::from(".effigy/reports/release/gates/floor.log")),
    };
    let plan = super::ReleasePreparePlan {
        repo_root: PathBuf::from("/tmp/fixture"),
        current_version: semver::Version::new(0, 1, 0),
        version_source: ResolvedVersionSource {
            path: PathBuf::from("VERSION"),
            kind: VersionFileKind::PlainText,
            field_path: None,
        },
        suggested_version: Some(semver::Version::new(0, 1, 1)),
        planned_version: Some(semver::Version::new(0, 1, 1)),
        suggested_tag: Some("v0.1.1".to_owned()),
        tag: Some("v0.1.1".to_owned()),
        version_override_used: false,
        release_date: "2026-09-05".to_owned(),
        gates_checked: true,
        configured_gate_count: 1,
        gate_results: vec![gate],
        environment_path: Some(PathBuf::from(
            ".effigy/reports/release/gates/environment.json",
        )),
        blockers: vec!["gate `floor` failed".to_owned()],
        mutations: Vec::new(),
        ready: false,
    };
    let rendered = render_release_prepare_plan_text(&plan);
    assert!(!rendered.contains("line-01"), "{rendered}");
    assert!(rendered.contains("line-02"), "{rendered}");
    assert!(rendered.contains("line-21"), "{rendered}");
    assert!(
        rendered.contains("log: .effigy/reports/release/gates/floor.log"),
        "{rendered}"
    );
    assert_eq!(
        rendered
            .lines()
            .filter(|line| line.contains("floor: fail"))
            .count(),
        1
    );
}

#[test]
fn gate_json_adds_optional_paths_without_changing_schema_id() {
    let root = temp_repo("gate-json");
    let report = run_release_gates(&root, &[shell_gate("smoke", "printf smoke-ok")], true);
    let json = render_release_gate_run_json(&collect_release_gate_run(root.clone(), 1, report));
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("json");
    assert_eq!(parsed["schema"], "effigy.release.gates.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert!(parsed["environment_path"].as_str().is_some());
    assert!(parsed["results"][0]["log_path"].as_str().is_some());
    assert_eq!(parsed["results"][0]["passed"], true);
}

#[test]
fn prepare_rolls_back_on_gate_failure_and_keeps_prepared_no() {
    let root = write_initial_release_repo("prepare-gate-fail", false);
    fs::write(
        root.join("effigy.toml"),
        "[release]\nversion-file = \"VERSION\"\nchangelog = \"CHANGELOG.md\"\n[release.gates.floor]\ncommand = \"printf floor-fail >&2; exit 4\"\n",
    )
    .expect("manifest");
    let prepared =
        execute_release_prepare(root.clone(), ".release-prepared.json", true, None, |_| {})
            .expect("prepare");
    assert!(!prepared.prepared);
    assert_eq!(
        fs::read_to_string(root.join("VERSION")).expect("version"),
        "0.1.0\n"
    );
    assert!(!root.join(".release-prepared.json").exists());
    let rendered = render_release_prepared_text(&prepared);
    assert!(rendered.contains("Prepared: no"), "{rendered}");
    assert!(rendered.contains("floor-fail"), "{rendered}");
    assert!(
        rendered.contains("log: ") && rendered.contains("floor.log"),
        "{rendered}"
    );
}

#[test]
fn redacted_environment_record_masks_token_like_keys() {
    let record = super::gate_reports::redacted_environment_record(
        "/bin/zsh",
        &PathBuf::from("/repo"),
        [
            ("PATH".to_owned(), "/bin".to_owned()),
            ("HOME".to_owned(), "/home/dev".to_owned()),
            ("CARGO_HOME".to_owned(), "/cargo".to_owned()),
            ("CARGO_REGISTRY_TOKEN".to_owned(), "secret".to_owned()),
            ("IGNORED".to_owned(), "nope".to_owned()),
        ],
    );
    assert_eq!(record["shell"], "/bin/zsh");
    assert_eq!(record["cwd"], "/repo");
    assert_eq!(record["PATH"], "/bin");
    assert_eq!(record["CARGO_HOME"], "/cargo");
    assert_eq!(record["CARGO_REGISTRY_TOKEN"], "<redacted>");
    assert!(record.get("IGNORED").is_none());
}
