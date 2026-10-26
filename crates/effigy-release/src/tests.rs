use super::prepare_helpers::unexpected_lockfile_change;
use super::{
    apply_release_mutations, collect_release_gate_run, collect_release_status,
    gate_outcome_word, git_create_tag, git_modified_files,
    is_optional_next_version_blocker, load_release_config, load_release_context,
    plan_release_pipeline, render_release_gate_run_json, render_release_gate_run_text,
    render_release_status_json, render_release_status_text, restore_mutation_snapshots,
    run::plan_prepare_mutations, run_release_gates, run_release_gates_with_lookup,
    run_release_gates_with_observer, run_release_gates_with_progress, snapshot_mutation_paths,
    test_support, FileMutationApply, FileMutationPlan, GateExecutionReport, GateObserver,
    GateResult, HostedEvidenceLookup, HostedEvidenceSpec, HostedRun, ResolvedGate,
};
use std::collections::BTreeMap;

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn temp_repo(name: &str) -> PathBuf {    effigy_test_support::Fixture::process_lifetime(&format!("effigy-release-{name}-"))
        .expect("fixture directory")
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
fn retired_catalog_pack_support_policy_key_is_refused() {
    let fixture = tempfile::tempdir().expect("private repository");
    let root = fixture.path();
    fs::write(
        root.join("effigy.toml"),
        "[release]\nversion-file = \"VERSION\"\nchangelog = \"CHANGELOG.md\"\nsync-catalog-pack-support-policy = \"support/catalog-pack-update.toml\"\n",
    )
    .expect("manifest");
    fs::write(root.join("VERSION"), "0.1.0\n").expect("version");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\n## [Unreleased]\n",
    )
    .expect("changelog");
    let error = load_release_config(root)
        .expect_err("retired key")
        .to_string();
    assert!(
        error.contains("sync-catalog-pack-support-policy` was removed in v0.15"),
        "{error}"
    );
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

    let plan = plan_release_pipeline(&root).expect("release plan");
    assert!(plan.ready, "{:?}", plan.blockers);
    let mutations = plan_prepare_mutations(&plan).expect("prepare mutations");
    assert_eq!(
        mutations
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

    let plan = plan_release_pipeline(&root).expect("release plan");
    assert!(plan.ready, "{:?}", plan.blockers);
    assert_eq!(
        plan.primary_identity().and_then(|identity| identity.next_version.clone()),
        Some(semver::Version::new(0, 3, 2))
    );
    let mutations = plan_prepare_mutations(&plan).expect("prepare mutations");

    let version_mutation = mutations
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
    for dependency in ["fixture-core", "fixture-renamed"] {
        assert!(
            version_mutation
                .detail_lines
                .iter()
                .any(|line| line.contains(dependency)),
            "{:?}",
            version_mutation.detail_lines
        );
    }
    assert!(mutations.iter().any(|mutation| matches!(
        mutation.apply,
        FileMutationApply::SyncCargoLock { ref workspace_version }
            if workspace_version == "0.3.2"
    )));

    let snapshots = snapshot_mutation_paths(&mutations).expect("mutation snapshots");
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

    let plan = plan_release_pipeline(&root).expect("release plan");
    assert!(plan.ready, "{:?}", plan.blockers);
    let mutations = plan_prepare_mutations(&plan).expect("prepare mutations");
    let package_mutation = mutations
        .iter()
        .find(|mutation| mutation.path == root.join("package.json"))
        .expect("package mutation");
    assert_eq!(package_mutation.kind, "sync-version-file");
    assert!(package_mutation
        .diff_preview
        .iter()
        .any(|line| line.contains("\"0.3.2\"")));

    apply_release_mutations(&root, &mutations).expect("apply release mutations");
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
    // Without the opt-in the release plans a bump, never the current version.
    assert_eq!(context.next_version, Some(semver::Version::new(0, 1, 1)));
    let plan = plan_release_pipeline(&root).expect("release plan");
    let app = plan.primary_identity().expect("semver identity");
    assert_eq!(app.next_version, Some(semver::Version::new(0, 1, 1)));
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
    let plan = plan_release_pipeline(&root).expect("release plan");
    let app = plan.primary_identity().expect("semver identity");
    assert_eq!(app.next_version, Some(semver::Version::new(0, 1, 1)));
}

#[test]
fn release_context_accepts_free_prose_in_release_sections() {
    let root = write_initial_release_repo("prose-sections", false);
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\n## [Unreleased]\n\nThe next release adds prose support.\n\n### Fixed\n- Follow-up\n\n## [0.1.0] - 2026-08-05\n\nReleased with a prose note.\n\n### Added\n- Initial release\n",
    )
    .expect("changelog");

    let context = load_release_context(&root).expect("release context");

    assert!(
        !context
            .blockers
            .iter()
            .any(|blocker| blocker.contains("changelog")),
        "prose must not block the release context: {:?}",
        context.blockers
    );
    assert_eq!(context.next_version, Some(semver::Version::new(0, 1, 1)));
}

#[test]
fn render_prepared_changelog_keeps_unreleased_prose() {
    let parsed = effigy_changelog::parse(
        "# Changelog\n\n## [Unreleased]\n\nA paragraph about the next release.\n\n### Fixed\n- Fix release output\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior fix\n",
    )
    .expect("parse changelog");

    let rendered = super::render_prepared_changelog_contents(
        &parsed,
        &semver::Version::new(0, 2, 5),
        "2026-03-11",
    )
    .expect("render changelog");

    let released = rendered
        .find("## [0.2.5] - 2026-03-11")
        .expect("released heading");
    let prose = rendered
        .find("A paragraph about the next release.")
        .expect("prose preserved");
    let category = rendered[released..]
        .find("### Fixed")
        .expect("category preserved")
        + released;
    assert!(
        released < prose && prose < category,
        "prose should stay under the promoted release: {rendered}"
    );
}

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
        task: None,
        description: None,
        stage: crate::GateStage::Certify,
        reuse_hosted_evidence: false,
        hosted_evidence: None,
    }
}

#[test]
fn failing_and_passing_gates_persist_logs_and_redacted_environment() {
    let root = temp_repo("gate-persist");
    let _env = crate::env_test_support::EnvGuard::set_many(&[(
        "CARGO_REGISTRY_TOKEN",
        Some(std::ffi::OsStr::new("super-secret-token")),
    )]);
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

    let passed_text = render_release_gate_run_text(&collect_release_gate_run(
        root.clone(),
        Vec::new(),
        passed_report,
    ));
    assert_eq!(
        passed_text
            .lines()
            .filter(|line| line.contains("ok: pass") || line.contains("] ok: pass"))
            .count(),
        1
    );
    assert!(!passed_text.contains("stdout:"), "{passed_text}");
}

/// A command plane draws live steps from the same facts the text lines carry.
#[derive(Default)]
struct RecordingGateObserver {
    events: Vec<String>,
}

impl GateObserver for RecordingGateObserver {
    fn gate_started(
        &mut self,
        position: usize,
        total: usize,
        gate: &ResolvedGate,
        reusing_hosted_evidence: bool,
    ) {
        self.events.push(format!(
            "start {position}/{total} {} reuse={reusing_hosted_evidence}",
            gate.name
        ));
    }

    fn gate_finished(&mut self, position: usize, total: usize, result: &GateResult) {
        self.events.push(format!(
            "finish {position}/{total} {} {}",
            result.name,
            gate_outcome_word(result)
        ));
    }

    fn note(&mut self, message: &str) {
        self.events.push(format!("note {message}"));
    }
}

#[test]
fn gate_observer_sees_each_gate_start_and_finish_in_order() {
    let root = temp_repo("gate-observer");
    let gates = vec![
        shell_gate("format", "printf check-ok"),
        shell_gate("smoke", "printf smoke-ok"),
    ];
    let mut observer = RecordingGateObserver::default();
    let report = run_release_gates_with_observer(&root, &gates, true, &mut observer);
    assert!(report.results.iter().all(|result| result.passed));
    assert_eq!(
        observer.events,
        [
            "start 1/2 format reuse=false",
            "finish 1/2 format passed",
            "start 2/2 smoke reuse=false",
            "finish 2/2 smoke passed",
        ]
    );
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
    assert!(!root.join(".gitignore").exists());
    assert!(fs::read_to_string(root.join(".git/info/exclude"))
        .expect("exclude")
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
fn status_text_shows_the_failed_gate_tail_and_log_path() {
    let root = write_initial_release_repo("failed-gate-tail", false);
    let mut stdout_lines = Vec::new();
    for index in 1..=21 {
        stdout_lines.push(format!("line-{index:02}"));
    }
    let report = GateExecutionReport {
        results: vec![GateResult {
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
            reused: false,
            hosted_run_url: None,
            hosted_head_sha: None,
            hosted_repository: None,
        }],
        ..GateExecutionReport::empty()
    };
    let plan = plan_release_pipeline(&root).expect("release plan");
    let status = collect_release_status(&plan, true, report);
    let rendered = render_release_status_text(&status);
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
    let gate = shell_gate("smoke", "printf smoke-ok");
    let report = run_release_gates(&root, std::slice::from_ref(&gate), true);
    let json =
        render_release_gate_run_json(&collect_release_gate_run(root.clone(), vec![gate], report));
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("json");
    assert_eq!(parsed["schema"], "effigy.release.gates.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert!(parsed["environment_path"].as_str().is_some());
    assert!(parsed["results"][0]["log_path"].as_str().is_some());
    assert_eq!(parsed["results"][0]["passed"], true);
}

/// The configured inventory and per-gate progress are what an operator needs
/// while a slow gate runs, so both are reported before and during the run and
/// mirrored in the JSON envelope.
#[test]
fn gate_run_reports_the_inventory_and_per_gate_progress() {
    let root = temp_repo("gate-inventory");
    let gates = vec![
        shell_gate("format", "printf check-ok"),
        shell_gate("smoke", "printf smoke-ok"),
    ];
    let mut progress = Vec::new();
    let report = run_release_gates_with_progress(&root, &gates, true, |line| {
        progress.push(line.to_owned());
    });
    assert!(report.results.iter().all(|result| result.passed));

    let start = progress
        .iter()
        .position(|line| line.contains("gate 1/2 `format`"))
        .expect("first gate start line");
    assert!(
        progress[start].contains("(certify) starting: printf check-ok"),
        "{}",
        progress[start]
    );
    let finish = progress
        .iter()
        .position(|line| line.contains("gate 1/2 `format` passed"))
        .expect("first gate finish line");
    assert!(finish > start);
    assert!(
        progress[finish].contains("ms)") || progress[finish].contains("s)"),
        "the finish line carries a duration: {}",
        progress[finish]
    );
    assert!(progress
        .iter()
        .any(|line| line.contains("gate 2/2 `smoke` (certify) starting")));

    let run = collect_release_gate_run(root.clone(), gates, report);
    let json = render_release_gate_run_json(&run);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("json");
    assert_eq!(parsed["gates"][0]["name"], "format");
    assert_eq!(parsed["gates"][0]["stage"], "certify");
    assert_eq!(parsed["gates"][0]["command"], "printf check-ok");
    assert_eq!(parsed["gates"][1]["name"], "smoke");
    assert_eq!(parsed["configured_gate_count"], 2);
    assert_eq!(parsed["executed_gate_count"], 2);
    assert!(parsed["results"][0]["duration_ms"].is_number());

    let text = render_release_gate_run_text(&run);
    assert!(text.contains("Configured Gates"), "{text}");
    assert!(
        text.contains("[1/2] format (certify) - printf check-ok"),
        "{text}"
    );
    assert!(
        text.contains("[2/2] smoke (certify) - printf smoke-ok"),
        "{text}"
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

struct StubHostedLookup {
    runs: Result<Vec<HostedRun>, String>,
}

impl HostedEvidenceLookup for StubHostedLookup {
    fn list_runs(&self, _query: &super::HostedEvidenceQuery) -> Result<Vec<HostedRun>, String> {
        self.runs.clone()
    }
}

fn hosted_spec() -> HostedEvidenceSpec {
    HostedEvidenceSpec {
        workflow: "ci.yml".to_owned(),
        event: "workflow_dispatch".to_owned(),
        branch: "main".to_owned(),
    }
}

fn reuse_gate(name: &str, command: &str) -> ResolvedGate {
    ResolvedGate {
        name: name.to_owned(),
        command: command.to_owned(),
        task: None,
        description: None,
        stage: crate::GateStage::Certify,
        reuse_hosted_evidence: true,
        hosted_evidence: Some(hosted_spec()),
    }
}

fn hosted_run(
    repo: &str,
    sha: &str,
    status: &str,
    conclusion: Option<&str>,
    url: &str,
) -> HostedRun {
    HostedRun {
        repository: repo.to_owned(),
        head_sha: sha.to_owned(),
        status: status.to_owned(),
        conclusion: conclusion.map(ToOwned::to_owned),
        url: url.to_owned(),
        database_id: Some(11),
        event: Some("workflow_dispatch".to_owned()),
        head_branch: Some("main".to_owned()),
        created_at: Some("2026-10-22T00:00:00Z".to_owned()),
    }
}

fn commit_and_set_origin(root: &PathBuf, origin: &str) -> String {
    for args in [
        &["config", "user.name", "Effigy Test"][..],
        &["config", "user.email", "effigy@example.invalid"][..],
        &["add", "VERSION", "CHANGELOG.md", "effigy.toml"][..],
        &["commit", "--quiet", "-m", "initial"][..],
        &["remote", "add", "origin", origin][..],
        &["branch", "-M", "main"][..],
    ] {
        let status = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .status()
            .expect("git fixture command");
        assert!(status.success(), "git command failed: {args:?}");
    }
    let head = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("head");
    String::from_utf8(head.stdout)
        .expect("head utf8")
        .trim()
        .to_owned()
}

#[test]
fn named_hosted_reuse_skips_local_command_and_keeps_unlisted_gate() {
    let root = write_initial_release_repo("hosted-reuse-ok", false);
    let sha = commit_and_set_origin(&root, "https://github.com/acme/demo.git");
    let lookup = StubHostedLookup {
        runs: Ok(vec![hosted_run(
            "acme/demo",
            &sha,
            "completed",
            Some("success"),
            "https://github.com/acme/demo/actions/runs/11",
        )]),
    };
    let report = run_release_gates_with_lookup(
        &root,
        &[
            shell_gate("smoke", "printf SMOKE_RAN > smoke.txt"),
            reuse_gate("test", "printf LOCAL_RAN > reused-local.txt"),
        ],
        true,
        &lookup,
        |_| {},
    );
    assert_eq!(report.results.len(), 2);
    assert!(report.results[0].passed);
    assert!(!report.results[0].reused);
    assert_eq!(
        fs::read_to_string(root.join("smoke.txt")).expect("smoke"),
        "SMOKE_RAN"
    );
    assert!(report.results[1].passed);
    assert!(report.results[1].reused);
    assert_eq!(
        report.results[1].hosted_run_url.as_deref(),
        Some("https://github.com/acme/demo/actions/runs/11")
    );
    assert!(!root.join("reused-local.txt").exists());
    let json = render_release_gate_run_json(&collect_release_gate_run(
        root.clone(),
        Vec::new(),
        report.clone(),
    ));
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("json");
    assert_eq!(parsed["results"][1]["reused"], true);
    assert_eq!(
        parsed["results"][1]["hosted_run_url"],
        "https://github.com/acme/demo/actions/runs/11"
    );
}

#[test]
fn unlisted_gate_still_runs_when_named_reuse_evidence_is_bad() {
    let root = write_initial_release_repo("hosted-reuse-unlisted", false);
    let sha = commit_and_set_origin(&root, "https://github.com/acme/demo.git");
    let lookup = StubHostedLookup {
        runs: Ok(vec![hosted_run(
            "acme/demo",
            "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
            "completed",
            Some("success"),
            "https://github.com/acme/demo/actions/runs/11",
        )]),
    };
    let report = run_release_gates_with_lookup(
        &root,
        &[
            shell_gate("smoke", "printf SMOKE_RAN > smoke.txt"),
            reuse_gate("test", "printf LOCAL_RAN > reused-local.txt"),
        ],
        false,
        &lookup,
        |_| {},
    );
    assert!(report.results[0].passed);
    assert!(!report.results[0].reused);
    assert_eq!(
        fs::read_to_string(root.join("smoke.txt")).expect("smoke"),
        "SMOKE_RAN"
    );
    assert!(!report.results[1].passed);
    assert!(!report.results[1].reused);
    assert!(
        report.results[1].stderr.contains("not the release commit"),
        "{}",
        report.results[1].stderr
    );
    assert!(report.results[1].stderr.contains(&sha));
    assert!(!root.join("reused-local.txt").exists());
}

#[test]
fn hosted_reuse_fails_closed_on_repository_mismatch_failed_and_incomplete_runs() {
    let root = write_initial_release_repo("hosted-reuse-fail-closed", false);
    let sha = commit_and_set_origin(&root, "https://github.com/acme/demo.git");

    let wrong_repo = run_release_gates_with_lookup(
        &root,
        &[reuse_gate("test", "printf LOCAL_RAN > reused-local.txt")],
        true,
        &StubHostedLookup {
            runs: Ok(vec![hosted_run(
                "other/repo",
                &sha,
                "completed",
                Some("success"),
                "https://github.com/other/repo/actions/runs/11",
            )]),
        },
        |_| {},
    );
    assert!(!wrong_repo.results[0].passed);
    assert!(wrong_repo.results[0]
        .stderr
        .contains("not this repository `acme/demo`"));

    let failed = run_release_gates_with_lookup(
        &root,
        &[reuse_gate("test", "printf LOCAL_RAN > reused-local.txt")],
        true,
        &StubHostedLookup {
            runs: Ok(vec![hosted_run(
                "acme/demo",
                &sha,
                "completed",
                Some("failure"),
                "https://github.com/acme/demo/actions/runs/11",
            )]),
        },
        |_| {},
    );
    assert!(!failed.results[0].passed);
    assert!(failed.results[0].stderr.contains("conclusion is `failure`"));

    let pending = run_release_gates_with_lookup(
        &root,
        &[reuse_gate("test", "printf LOCAL_RAN > reused-local.txt")],
        true,
        &StubHostedLookup {
            runs: Ok(vec![hosted_run(
                "acme/demo",
                &sha,
                "in_progress",
                None,
                "https://github.com/acme/demo/actions/runs/11",
            )]),
        },
        |_| {},
    );
    assert!(!pending.results[0].passed);
    assert!(pending.results[0].stderr.contains("is pending"));
    assert!(!root.join("reused-local.txt").exists());
}

#[test]
fn reuse_without_hosted_evidence_workflow_fails_at_resolve() {
    let root = temp_repo("hosted-reuse-missing-workflow");
    fs::write(
        root.join("effigy.toml"),
        r#"
[release]
version-file = "VERSION"
changelog = "CHANGELOG.md"

[release.gates.test]
command = "cargo test"
reuse-hosted-evidence = true
"#,
    )
    .expect("manifest");
    fs::write(root.join("VERSION"), "0.1.0\n").expect("version");
    fs::write(root.join("CHANGELOG.md"), "# Changelog\n").expect("changelog");
    let error = load_release_config(&root).expect_err("missing workflow");
    assert!(
        error
            .to_string()
            .contains("[release.hosted-evidence].workflow is not configured"),
        "{error}"
    );
}

fn write_empty_unreleased_repo(name: &str) -> PathBuf {
    let root = temp_repo(name);
    fs::write(
        root.join("effigy.toml"),
        "[release]\nversion-file = \"VERSION\"\nchangelog = \"CHANGELOG.md\"\n",
    )
    .expect("manifest");
    fs::write(root.join("VERSION"), "0.2.1\n").expect("version");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\n## [Unreleased]\n\n## [0.2.1] - 2026-09-01\n\n### Fixed\n- Prior fix\n",
    )
    .expect("changelog");
    root
}

#[test]
fn status_gate_check_passes_with_empty_unreleased_and_reports_no_next_version() {
    let root = write_empty_unreleased_repo("status-empty-unreleased");
    let context = load_release_context(&root).expect("release context");
    assert!(context.unreleased_empty);
    assert_eq!(context.next_version, None);
    assert!(
        context
            .blockers
            .iter()
            .any(|blocker| blocker.contains("unreleased changelog section has no entries")),
        "{:?}",
        context.blockers
    );

    let report = run_release_gates(&root, &[shell_gate("ok", "true")], true);
    assert!(report.results.iter().all(|gate| gate.passed));
    let status = collect_release_status(
        &crate::plan_release_pipeline(&root).expect("release plan"),
        true,
        report,
    );
    assert!(status.gates_passed);
    assert!(status.gate_check_passed());
    assert!(!status.ready);
    assert_eq!(status.next_version, None);
    assert!(
        status
            .blockers
            .iter()
            .any(|blocker| blocker.contains("unreleased changelog section has no entries")),
        "{:?}",
        status.blockers
    );

    let parsed: serde_json::Value =
        serde_json::from_str(&render_release_status_json(&status)).expect("json");
    assert_eq!(parsed["schema"], "effigy.release.status.v1");
    assert_eq!(parsed["ready"], false);
    assert_eq!(parsed["gates_passed"], true);
    assert_eq!(parsed["gate_check_passed"], true);
    assert_eq!(parsed["next_version"], serde_json::Value::Null);
    assert_eq!(parsed["unreleased"]["empty"], true);

    let text = render_release_status_text(&status);
    assert!(text.starts_with("Release Status\n"), "{text}");
    assert!(text.contains("Gate check: passed"), "{text}");
    assert!(text.contains("Next version: unavailable"), "{text}");
    assert!(text.contains("Ready to run: no"), "{text}");
}

#[test]
fn status_gate_check_fails_when_a_gate_fails() {
    let root = write_empty_unreleased_repo("status-gate-fails");

    let report = run_release_gates(&root, &[shell_gate("smoke", "exit 1")], true);
    assert!(!report.results[0].passed);
    let status = collect_release_status(
        &crate::plan_release_pipeline(&root).expect("release plan"),
        true,
        report,
    );
    assert!(!status.gates_passed);
    assert!(!status.gate_check_passed());
    assert!(!status.ready);

    let text = render_release_status_text(&status);
    assert!(text.starts_with("Release Status Blocked\n"), "{text}");
    assert!(text.contains("Gate check: failed"), "{text}");
}

#[test]
fn status_gate_check_still_fails_for_non_version_blockers() {
    // A version mismatch is a real release-input failure, not an absent
    // proposal: passing gates must not excuse it.
    let root = write_empty_unreleased_repo("status-version-mismatch");
    fs::write(root.join("VERSION"), "0.2.2\n").expect("version");
    let context = load_release_context(&root).expect("release context");
    assert!(
        context
            .blockers
            .iter()
            .any(|blocker| blocker.contains("version file reports")),
        "{:?}",
        context.blockers
    );

    let report = run_release_gates(&root, &[shell_gate("ok", "true")], true);
    let status = collect_release_status(
        &crate::plan_release_pipeline(&root).expect("release plan"),
        true,
        report,
    );
    assert!(status.gates_passed);
    assert!(!status.gate_check_passed());
    assert!(!status.ready);
}

#[test]
fn an_empty_unreleased_section_blocks_the_release_plan() {
    let root = write_empty_unreleased_repo("prepare-empty-unreleased");
    let plan = plan_release_pipeline(&root).expect("release plan");
    assert!(!plan.ready);
    assert!(
        plan.blockers
            .iter()
            .any(|blocker| blocker.contains("unreleased changelog section has no entries")),
        "{:?}",
        plan.blockers
    );
}

#[test]
fn optional_next_version_blocker_matches_only_absent_proposals() {
    assert!(is_optional_next_version_blocker(
        "unreleased changelog section has no entries"
    ));
    assert!(is_optional_next_version_blocker(
        "no next version could be derived from changelog content"
    ));
    assert!(!is_optional_next_version_blocker("gate `smoke` failed"));
    assert!(!is_optional_next_version_blocker(
        "version file reports 0.2.2 but latest changelog release is 0.2.1"
    ));
}

#[test]
fn status_gate_check_is_not_passed_when_gates_were_not_checked() {
    let root = write_empty_unreleased_repo("status-gates-unchecked");

    let status = collect_release_status(
        &crate::plan_release_pipeline(&root).expect("release plan"),
        false,
        GateExecutionReport::empty(),
    );
    assert!(!status.gates_checked);
    assert!(!status.gates_passed);
    assert!(!status.gate_check_passed());
    assert!(!status.ready);

    let parsed: serde_json::Value =
        serde_json::from_str(&render_release_status_json(&status)).expect("json");
    assert_eq!(parsed["gates_passed"], false);
    assert_eq!(parsed["gate_check_passed"], false);

    let text = render_release_status_text(&status);
    assert!(text.starts_with("Release Status Blocked\n"), "{text}");
    assert!(!text.contains("Gate check: passed"), "{text}");
}

fn suggested_bump_for(
    changelog_markdown: &str,
    current_version: &str,
    pre_1_0: bool,
) -> super::BumpKind {
    let changelog =
        effigy_changelog::parse(changelog_markdown).expect("fixture changelog should parse");
    super::prepare_helpers::suggested_bump(
        &changelog,
        &semver::Version::parse(current_version).expect("fixture version"),
        pre_1_0,
    )
}

#[test]
fn proposed_bump_removed_category_is_minor_under_pre_1_0() {
    let changelog = "# Changelog\n\n## [Unreleased]\n\n### Removed\n- Removed public API\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n";
    assert_eq!(
        suggested_bump_for(changelog, "0.2.4", true),
        super::BumpKind::Minor
    );
}

#[test]
fn proposed_bump_fixed_only_category_stays_patch() {
    let changelog = "# Changelog\n\n## [Unreleased]\n\n### Fixed\n- Compatible fix\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n";
    assert_eq!(
        suggested_bump_for(changelog, "0.2.4", true),
        super::BumpKind::Patch
    );
}

#[test]
fn proposed_bump_mixed_removed_and_fixed_is_minor() {
    let changelog = "# Changelog\n\n## [Unreleased]\n\n### Removed\n- Removed public API\n\n### Fixed\n- Compatible fix\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n";
    assert_eq!(
        suggested_bump_for(changelog, "0.2.4", true),
        super::BumpKind::Minor
    );
}

#[test]
fn proposed_bump_removed_outside_pre_1_0_forces_major() {
    let changelog = "# Changelog\n\n## [Unreleased]\n\n### Removed\n- Removed public API\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n";
    assert_eq!(
        suggested_bump_for(changelog, "0.2.4", false),
        super::BumpKind::Major
    );
}

#[test]
fn proposed_bump_normal_1_x_release_is_unchanged() {
    let added = "# Changelog\n\n## [Unreleased]\n\n### Added\n- New feature\n\n## [1.4.2] - 2026-03-10\n\n### Fixed\n- Prior release\n";
    assert_eq!(
        suggested_bump_for(added, "1.4.2", true),
        super::BumpKind::Minor
    );

    let fixed = "# Changelog\n\n## [Unreleased]\n\n### Fixed\n- Compatible fix\n\n## [1.4.2] - 2026-03-10\n\n### Fixed\n- Prior release\n";
    assert_eq!(
        suggested_bump_for(fixed, "1.4.2", true),
        super::BumpKind::Patch
    );

    let removed = "# Changelog\n\n## [Unreleased]\n\n### Removed\n- Removed public API\n\n## [1.4.2] - 2026-03-10\n\n### Fixed\n- Prior release\n";
    assert_eq!(
        suggested_bump_for(removed, "1.4.2", true),
        super::BumpKind::Major
    );
}

fn write_nested_lockfile_release_repo(name: &str, sync_files: &str) -> PathBuf {
    let root = temp_repo(name);
    fs::write(
        root.join("effigy.toml"),
        format!(
            "[release]\nversion-file = \"Cargo.toml\"\nversion-path = \
             \"workspace.package.version\"\nchangelog = \"CHANGELOG.md\"\nsync-files = \
             [{sync_files}]\n"
        ),
    )
    .expect("manifest");
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/*\"]\nexclude = [\"prototypes/fixture\"]\n\
         resolver = \"2\"\n\n[workspace.package]\nversion = \"0.3.1\"\nedition = \"2021\"\n",
    )
    .expect("cargo manifest");
    fs::create_dir_all(root.join("crates/core/src")).expect("member dir");
    fs::write(root.join("crates/core/src/lib.rs"), "pub fn core() {}\n").expect("member source");
    fs::write(
        root.join("crates/core/Cargo.toml"),
        "[package]\nname = \"fixture-core\"\nversion.workspace = true\nedition = \"2021\"\n",
    )
    .expect("member manifest");
    fs::create_dir_all(root.join("prototypes/fixture/src")).expect("prototype dir");
    fs::write(
        root.join("prototypes/fixture/src/main.rs"),
        "fn main() {}\n",
    )
    .expect("prototype source");
    fs::write(
        root.join("prototypes/fixture/Cargo.toml"),
        "[package]\nname = \"fixture-proto\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
         [workspace]\n\n[dependencies]\nfixture-core = { path = \"../../crates/core\", version = \
         \"0.3.1\" }\n",
    )
    .expect("prototype manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\n## [Unreleased]\n\n### Fixed\n- Refresh the excluded lockfile\n",
    )
    .expect("changelog");
    root
}

#[test]
fn release_config_accepts_a_declared_nested_workspace_excluded_lockfile() {
    let root = write_nested_lockfile_release_repo(
        "nested-lock-accepted",
        "\"prototypes/fixture/Cargo.lock\"",
    );
    fs::write(
        root.join("prototypes/fixture/Cargo.lock"),
        "version = 4\n\n[[package]]\nname = \"fixture-core\"\nversion = \"0.3.1\"\n",
    )
    .expect("lockfile");

    let config = load_release_config(&root).expect("nested lockfile should be a sync target");
    assert_eq!(
        config
            .sync_files
            .iter()
            .map(|sync| (sync.path.clone(), sync.kind))
            .collect::<Vec<_>>(),
        vec![(
            root.join("prototypes/fixture/Cargo.lock"),
            super::SyncFileKind::CargoLock
        )]
    );

    fs::remove_dir_all(&root).ok();
}

#[test]
fn release_config_refuses_sync_files_that_leave_the_release_root() {
    let root = write_nested_lockfile_release_repo("nested-lock-escape", "\"../Cargo.lock\"");
    let error = load_release_config(&root).expect_err("`..` must be refused");
    assert!(
        error.to_string().contains("must not contain `..`"),
        "{error}"
    );

    let probe = temp_repo("nested-lock-absolute-probe");
    let absolute = format!("\"{}\"", probe.join("Cargo.lock").display());
    let root = write_nested_lockfile_release_repo("nested-lock-absolute", &absolute);
    let error = load_release_config(&root).expect_err("an absolute path must be refused");
    assert!(
        error
            .to_string()
            .contains("must be relative to the release root"),
        "{error}"
    );

    fs::remove_dir_all(&root).ok();
}

#[test]
fn release_config_refuses_a_symlinked_sync_directory_outside_the_root() {
    let root = write_nested_lockfile_release_repo("nested-lock-symlink", "\"linked/Cargo.lock\"");
    let outside = temp_repo("nested-lock-symlink-outside");
    fs::write(outside.join("Cargo.toml"), "[workspace]\n").expect("outside manifest");
    fs::write(outside.join("Cargo.lock"), "version = 4\n").expect("outside lockfile");
    std::os::unix::fs::symlink(&outside, root.join("linked")).expect("symlink");

    let error = load_release_config(&root).expect_err("a symlinked directory must be refused");
    assert!(
        error.to_string().contains("outside the release root"),
        "{error}"
    );

    fs::remove_dir_all(&root).ok();
    fs::remove_dir_all(&outside).ok();
}

#[test]
fn release_config_refuses_a_nested_lockfile_without_a_sibling_manifest() {
    let error = load_release_config(&write_nested_lockfile_release_repo(
        "nested-lock-no-manifest",
        "\"prototypes/fixture/nested/Cargo.lock\"",
    ))
    .expect_err("an unowned lockfile must be refused");
    assert!(
        error.to_string().contains("has no Cargo.toml beside it"),
        "{error}"
    );
}

#[test]
fn prepare_plan_reports_the_nested_sync_workspace() {
    let root = write_nested_lockfile_release_repo(
        "nested-lock-prepare-plan",
        "\"prototypes/fixture/Cargo.lock\"",
    );
    fs::write(
        root.join("prototypes/fixture/Cargo.lock"),
        "version = 4\n\n[[package]]\nname = \"fixture-core\"\nversion = \"0.3.1\"\n",
    )
    .expect("lockfile");

    let plan = plan_release_pipeline(&root).expect("release plan");
    let mutations = plan_prepare_mutations(&plan).expect("prepare mutations");
    let mutation = mutations
        .iter()
        .find(|mutation| mutation.path == root.join("prototypes/fixture/Cargo.lock"))
        .expect("nested lockfile mutation");
    assert!(
        mutation.summary.contains("prototypes/fixture/Cargo.lock"),
        "{}",
        mutation.summary
    );
    assert!(mutation
        .detail_lines
        .contains(&"sync workspace: prototypes/fixture".to_owned()));
    assert!(mutation
        .detail_lines
        .iter()
        .any(|line| line.contains("cargo update --quiet -p <release package>")));

    fs::remove_dir_all(&root).ok();
}

#[test]
fn sync_refuses_a_lockfile_owned_by_another_workspace() {
    let root = write_nested_lockfile_release_repo("nested-lock-foreign-owner", "\"Cargo.lock\"");
    fs::write(
        root.join("crates/core/Cargo.lock"),
        "version = 4\n\n[[package]]\nname = \"fixture-core\"\nversion = \"0.3.1\"\n",
    )
    .expect("member lockfile");
    let mutation = FileMutationPlan {
        path: root.join("crates/core/Cargo.lock"),
        kind: "sync-file",
        summary: String::new(),
        before_preview: String::new(),
        after_preview: String::new(),
        detail_lines: Vec::new(),
        diff_preview: Vec::new(),
        apply: FileMutationApply::SyncCargoLock {
            workspace_version: "0.3.2".to_owned(),
        },
    };

    let error = apply_release_mutations(&root, &[mutation]).expect_err("foreign lockfile");
    assert!(error.to_string().contains("owns that lockfile"), "{error}");
    assert_eq!(
        fs::read_to_string(root.join("crates/core/Cargo.lock")).expect("lockfile"),
        "version = 4\n\n[[package]]\nname = \"fixture-core\"\nversion = \"0.3.1\"\n"
    );

    fs::remove_dir_all(&root).ok();
}

#[test]
fn sync_moves_release_packages_in_a_workspace_excluded_lockfile() {
    let root =
        write_nested_lockfile_release_repo("nested-lock-sync", "\"prototypes/fixture/Cargo.lock\"");
    let lockfile_before = "# This file is automatically @generated by Cargo.\n\
                          # It is not intended for manual editing.\nversion = 4\n\n\
                          [[package]]\nname = \"fixture-core\"\n\
                          version = \"0.3.1\"\n\n[[package]]\nname = \"fixture-proto\"\n\
                          version = \"0.1.0\"\ndependencies = [\n \"fixture-core\",\n]\n";
    fs::write(root.join("prototypes/fixture/Cargo.lock"), lockfile_before).expect("lockfile");
    // The version bump runs before the sync in the prepare stage.
    fs::write(
        root.join("crates/core/Cargo.toml"),
        "[package]\nname = \"fixture-core\"\nversion = \"0.3.2\"\nedition = \"2021\"\n",
    )
    .expect("bumped member manifest");
    let root_lock_before =
        fs::read_to_string(root.join("Cargo.lock")).unwrap_or_else(|_| "<absent>".to_owned());
    let mutation = FileMutationPlan {
        path: root.join("prototypes/fixture/Cargo.lock"),
        kind: "sync-file",
        summary: String::new(),
        before_preview: String::new(),
        after_preview: String::new(),
        detail_lines: Vec::new(),
        diff_preview: Vec::new(),
        apply: FileMutationApply::SyncCargoLock {
            workspace_version: "0.3.2".to_owned(),
        },
    };

    apply_release_mutations(&root, &[mutation]).expect("nested lockfile sync");
    let lockfile =
        fs::read_to_string(root.join("prototypes/fixture/Cargo.lock")).expect("nested lockfile");
    assert!(
        lockfile.contains("name = \"fixture-core\"\nversion = \"0.3.2\""),
        "{lockfile}"
    );
    assert!(
        lockfile.contains("name = \"fixture-proto\"\nversion = \"0.1.0\""),
        "an unrelated package version must not move: {lockfile}"
    );
    assert_eq!(
        fs::read_to_string(root.join("Cargo.lock")).unwrap_or_else(|_| "<absent>".to_owned()),
        root_lock_before,
        "the release root's own lockfile must stay untouched"
    );

    fs::remove_dir_all(&root).ok();
}

#[test]
fn nested_lockfile_change_allows_a_partial_workspace_record() {
    let release_members = BTreeMap::from([("fixture-core".to_owned(), "0.3.2".to_owned())]);
    let before = "[[package]]\nname = \"fixture-core\"\nversion = \"0.3.1\"\n\n\
                  [[package]]\nname = \"fixture-proto\"\nversion = \"0.1.0\"\n\n\
                  [[package]]\nname = \"serde\"\nversion = \"1.0.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n";
    let after = before.replacen("version = \"0.3.1\"", "version = \"0.3.2\"", 1);

    // Only the recorded release package may move, and only to its metadata
    // version; the excluded consumer's own package stays where it is.
    assert_eq!(
        super::prepare_helpers::unexpected_lockfile_change_with(
            before,
            &after,
            &release_members,
            false,
        ),
        None
    );
    let refused = super::prepare_helpers::unexpected_lockfile_change_with(
        before,
        &after.replace(
            "name = \"fixture-proto\"\nversion = \"0.1.0\"",
            "name = \"fixture-proto\"\nversion = \"0.2.0\"",
        ),
        &release_members,
        false,
    )
    .expect("an unrelated package move must still be refused");
    assert!(
        refused.contains("outside actual workspace member"),
        "{refused}"
    );
}
