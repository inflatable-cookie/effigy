use super::{
    compare_release_state_fingerprints, detect_version_file_kind, format_release_tag,
    gate_blockers, load_release_config, load_release_prepared_state, normalized_expected_files,
    resolve_version_field_path, snapshot_mutation_paths, write_release_prepared_state,
    FileMutationApply, FileMutationPlan, GateExecutionReport, GateResult,
    ReleasePreparedFileFingerprint, ReleasePreparedSourceFingerprints, VersionFileKind,
};
use std::fs;
use std::path::PathBuf;

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

#[test]
fn version_file_kind_detection_matches_supported_names() {
    assert_eq!(
        detect_version_file_kind(std::path::Path::new("Cargo.toml")),
        Some(VersionFileKind::CargoToml)
    );
    assert_eq!(
        detect_version_file_kind(std::path::Path::new("package.json")),
        Some(VersionFileKind::PackageJson)
    );
    assert_eq!(
        detect_version_file_kind(std::path::Path::new("pyproject.toml")),
        Some(VersionFileKind::PyProjectToml)
    );
    assert_eq!(
        detect_version_file_kind(std::path::Path::new("VERSION")),
        Some(VersionFileKind::PlainText)
    );
}

#[test]
fn version_field_path_defaults_follow_known_formats() {
    assert_eq!(
        resolve_version_field_path(VersionFileKind::CargoToml, None).expect("default path"),
        Some("package.version".to_owned())
    );
    assert_eq!(
        resolve_version_field_path(VersionFileKind::PackageJson, None).expect("default path"),
        Some("version".to_owned())
    );
    assert_eq!(
        resolve_version_field_path(VersionFileKind::PyProjectToml, None).expect("default path"),
        None
    );
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

    write_release_prepared_state(
        &state_path,
        &root,
        &semver::Version::parse("0.2.9").expect("previous"),
        Some(&semver::Version::parse("0.3.0").expect("suggested")),
        Some(&semver::Version::parse("0.3.0").expect("prepared")),
        Some("v0.3.0"),
        Some("v0.3.0"),
        false,
        "2026-04-16",
        true,
        std::slice::from_ref(&version_file),
        Some("main"),
        Some("deadbeef"),
    )
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

#[test]
fn normalized_expected_files_adds_state_file_once() {
    let repo_root = PathBuf::from("/tmp/repo");
    let files = vec![
        repo_root.join("Cargo.toml"),
        repo_root.join("CHANGELOG.md"),
        repo_root.join("Cargo.toml"),
    ];
    let normalized = normalized_expected_files(".release-prepared.json", &repo_root, &files);
    assert_eq!(
        normalized,
        vec![
            ".release-prepared.json".to_owned(),
            "CHANGELOG.md".to_owned(),
            "Cargo.toml".to_owned(),
        ]
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
