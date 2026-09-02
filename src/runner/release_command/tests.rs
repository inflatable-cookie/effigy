use super::{
    load_release_config, parse_prepare_mutation_inspection_request, remediation_hints_for_blockers,
    resolve_verify_install_repo_url, validate_prepare_version_override, ReleaseBlockedStage,
};
use effigy_core::resolver::ResolvedTarget;
#[path = "../../../crates/effigy-release/src/test_support.rs"]
mod release_test_support;
use effigy_release::{
    build_diff_preview, detect_cargo_version_path, detect_pyproject_version_path,
    detect_version_file_kind, format_release_tag, json_value_at_path,
    normalize_verify_install_repo_url, parse_indexed_review_inspection_request,
    render_changelog_preview_line as changelog_preview_line, render_execute_review_menu_lines,
    render_prepare_review_menu_lines, render_prepared_changelog_contents,
    render_updated_version_contents, replace_json_string_at_path_preserving_layout,
    resolve_version_field_path, review_label, suggested_bump, toml_value_at_path, BumpKind,
    ExecuteReviewState, PrepareReviewState, ReleaseConfig, ReleaseContext, ReleaseExecutePlan,
    ReleasePreparePlan, ResolvedVersionSource, SyncFileKind, VersionFileKind,
};
use effigy_tasks::ResolutionMode;

#[test]
fn version_file_kind_detection_matches_supported_names() {
    release_test_support::assert_supported_version_file_kinds();
}

#[test]
fn version_field_path_defaults_follow_known_formats() {
    release_test_support::assert_default_version_field_paths();
}

#[test]
fn detect_cargo_version_path_supports_workspace_inherited_versions() {
    release_test_support::assert_workspace_inherited_cargo_version_path();
}

#[test]
fn toml_and_json_path_helpers_follow_dot_segments() {
    let toml: toml::Value = toml::from_str("[package]\nversion = \"0.2.4\"\n").expect("toml");
    let json: serde_json::Value =
        serde_json::from_str("{\"package\":{\"version\":\"0.2.4\"}}").expect("json");

    assert_eq!(
        toml_value_at_path(&toml, "package.version").and_then(toml::Value::as_str),
        Some("0.2.4")
    );
    assert_eq!(
        json_value_at_path(&json, "package.version").and_then(serde_json::Value::as_str),
        Some("0.2.4")
    );
}

#[test]
fn detect_pyproject_path_prefers_project_version_when_present() {
    let parsed: toml::Value = toml::from_str("[project]\nversion = \"0.2.4\"\n").expect("toml");
    assert_eq!(
        detect_pyproject_version_path(&parsed),
        Some("project.version")
    );
}

#[test]
fn render_updated_version_contents_supports_json_and_plain_text() {
    let root = std::env::temp_dir().join(format!(
        "effigy-release-version-render-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("mkdir");

    let package_json = root.join("package.json");
    std::fs::write(&package_json, "{\n  \"version\": \"0.2.4\"\n}\n").expect("write json");
    let version_file = root.join("VERSION");
    std::fs::write(&version_file, "0.2.4\n").expect("write version");

    let updated_json = render_updated_version_contents(
        &ResolvedVersionSource {
            path: package_json,
            kind: VersionFileKind::PackageJson,
            field_path: Some("version".to_owned()),
        },
        &semver::Version::new(0, 2, 5),
    )
    .expect("render json");
    let updated_text = render_updated_version_contents(
        &ResolvedVersionSource {
            path: version_file,
            kind: VersionFileKind::PlainText,
            field_path: None,
        },
        &semver::Version::new(0, 2, 5),
    )
    .expect("render version");

    assert!(updated_json.contains("\"version\": \"0.2.5\""));
    assert_eq!(updated_text, "0.2.5\n");
}

#[test]
fn render_updated_version_contents_preserves_toml_comments_and_order() {
    let root = std::env::temp_dir().join(format!(
        "effigy-release-version-render-toml-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("mkdir");

    let cargo_toml = root.join("Cargo.toml");
    std::fs::write(
        &cargo_toml,
        "# leading comment\n[package] # keep package heading comment\nname = \"fixture\"\nversion = \"0.2.4\" # inline version note\nedition = \"2021\"\n\n[dependencies]\nserde = \"1\"\n",
    )
    .expect("write cargo");

    let updated = render_updated_version_contents(
        &ResolvedVersionSource {
            path: cargo_toml,
            kind: VersionFileKind::CargoToml,
            field_path: Some("package.version".to_owned()),
        },
        &semver::Version::new(0, 2, 5),
    )
    .expect("render cargo");

    assert!(updated.contains("# leading comment"));
    assert!(updated.contains("[package] # keep package heading comment"));
    assert!(updated.contains("version = \"0.2.5\" # inline version note"));
    assert!(updated.contains("\n\n[dependencies]\nserde = \"1\"\n"));
}

#[test]
fn render_updated_version_contents_preserves_pyproject_comments() {
    let root = std::env::temp_dir().join(format!(
        "effigy-release-version-render-pyproject-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("mkdir");

    let pyproject = root.join("pyproject.toml");
    std::fs::write(
        &pyproject,
        "# pyproject comment\n[project]\nname = \"fixture\"\nversion = \"0.2.4\" # keep comment\n\n[tool.poetry]\nversion = \"9.9.9\"\n",
    )
    .expect("write pyproject");

    let updated = render_updated_version_contents(
        &ResolvedVersionSource {
            path: pyproject,
            kind: VersionFileKind::PyProjectToml,
            field_path: None,
        },
        &semver::Version::new(0, 2, 5),
    )
    .expect("render pyproject");

    assert!(updated.contains("# pyproject comment"));
    assert!(updated.contains("version = \"0.2.5\" # keep comment"));
    assert!(updated.contains("[tool.poetry]\nversion = \"9.9.9\""));
}

#[test]
fn replace_json_string_at_path_preserves_layout_for_nested_version_keys() {
    let updated = replace_json_string_at_path_preserving_layout(
        "{\n  \"package\": {\n    \"name\": \"fixture\",\n    \"version\"  :  \"0.2.4\"\n  },\n  \"unchanged\": [1, {\"flag\": true}]\n}\n",
        "package.version",
        "0.2.5",
    )
    .expect("replace nested json value");

    assert!(updated.contains("\"version\"  :  \"0.2.5\""));
    assert!(updated.contains("\"unchanged\": [1, {\"flag\": true}]"));
}

#[test]
fn render_prepared_changelog_moves_unreleased_entries_into_new_release() {
    let parsed = effigy_changelog::parse(
        "# Changelog\n\n## [Unreleased]\n\n### Fixed\n- Fix release output\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior fix\n",
    )
    .expect("parse changelog");
    let rendered =
        render_prepared_changelog_contents(&parsed, &semver::Version::new(0, 2, 5), "2026-03-11")
            .expect("render changelog");

    assert!(rendered.contains("## [Unreleased]"));
    assert!(rendered.contains("## [0.2.5] - 2026-03-11"));
    assert_eq!(
        changelog_preview_line(&rendered, &semver::Version::new(0, 2, 5), "2026-03-11"),
        "## [0.2.5] - 2026-03-11"
    );
}

#[test]
fn suggested_bump_respects_pre_1_0_breaking_policy() {
    let changelog = effigy_changelog::parse(
        "# Changelog\n\n## [Unreleased]\n\n### Breaking\n- Break\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior fix\n",
    )
    .expect("parse changelog");

    assert_eq!(
        suggested_bump(&changelog, &semver::Version::new(0, 2, 4), true),
        BumpKind::Minor
    );
    assert_eq!(
        suggested_bump(&changelog, &semver::Version::new(0, 2, 4), false),
        BumpKind::Major
    );
}

#[test]
fn validate_prepare_version_override_rejects_non_incrementing_versions() {
    let changelog = effigy_changelog::parse(
        "# Changelog\n\n## [Unreleased]\n\n### Fixed\n- Fix release output\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior fix\n",
    )
    .expect("parse changelog");
    let mut context = ReleaseContext {
        repo_root: std::env::temp_dir(),
        config: ReleaseConfig {
            version_source: ResolvedVersionSource {
                path: std::env::temp_dir().join("Cargo.toml"),
                kind: VersionFileKind::CargoToml,
                field_path: Some("package.version".to_owned()),
            },
            changelog_path: std::env::temp_dir().join("CHANGELOG.md"),
            pre_1_0: false,
            initial_tag_current_version: false,
            sync_files: Vec::new(),
            gates: Vec::new(),
            tag_format: "v{version}".to_owned(),
        },
        current_version: semver::Version::new(0, 2, 4),
        parsed_changelog: changelog,
        changelog_diagnostics: Vec::new(),
        unreleased_counts: std::collections::BTreeMap::new(),
        unreleased_empty: false,
        suggested_bump: BumpKind::Patch,
        next_version: Some(semver::Version::new(0, 2, 5)),
        tag: Some(format_release_tag(
            "v{version}",
            &semver::Version::new(0, 2, 5),
        )),
        blockers: Vec::new(),
    };

    let err = validate_prepare_version_override(&context, "0.2.4")
        .expect_err("current version should be rejected");
    assert!(err.contains("must be greater than current version"));

    context.config.initial_tag_current_version = true;
    context.parsed_changelog =
        effigy_changelog::parse("# Changelog\n\n## [Unreleased]\n\n### Fixed\n- First release\n")
            .expect("parse initial changelog");
    context.next_version = Some(semver::Version::new(0, 2, 4));
    assert_eq!(
        validate_prepare_version_override(&context, "0.2.4")
            .expect("initial current version should be accepted"),
        semver::Version::new(0, 2, 4)
    );
}

#[test]
fn build_diff_preview_limits_to_concise_changed_lines() {
    let before = "alpha\nbeta\ncharlie\ndelta\necho\nfoxtrot\ngolf\n";
    let after =
        "alpha\nbeta changed\ncharlie\ndelta changed\necho\nfoxtrot changed\ngolf changed\n";

    let preview = build_diff_preview(before, after);

    assert_eq!(
        preview,
        vec![
            "- beta".to_owned(),
            "+ beta changed".to_owned(),
            "- delta".to_owned(),
            "+ delta changed".to_owned(),
            "- foxtrot".to_owned(),
            "+ foxtrot changed".to_owned(),
            "... 1 more changed line(s)".to_owned(),
        ]
    );
}

#[test]
fn parse_prepare_mutation_inspection_request_accepts_keyword_and_bare_index() {
    assert_eq!(
        parse_prepare_mutation_inspection_request("inspect 2", 3),
        Some(1)
    );
    assert_eq!(parse_prepare_mutation_inspection_request("3", 3), Some(2));
    assert_eq!(
        parse_prepare_mutation_inspection_request("inspect 4", 3),
        None
    );
    assert_eq!(
        parse_prepare_mutation_inspection_request("inspect nope", 3),
        None
    );
}

#[test]
fn parse_indexed_review_inspection_request_accepts_short_form() {
    assert_eq!(parse_indexed_review_inspection_request("i 1", 2), Some(0));
    assert_eq!(parse_indexed_review_inspection_request("2", 2), Some(1));
    assert_eq!(parse_indexed_review_inspection_request("0", 2), None);
}

#[test]
fn review_label_marks_pending_reviewed_and_not_applicable() {
    assert_eq!(review_label(false, true), "pending");
    assert_eq!(review_label(true, true), "reviewed");
    assert_eq!(review_label(false, false), "n/a");
}

#[test]
fn remediation_hints_cover_prepare_and_execute_blockers() {
    let prepare_hints = remediation_hints_for_blockers(
        &[
            "unreleased changelog section has no entries".to_owned(),
            "gate `smoke` failed".to_owned(),
        ],
        ReleaseBlockedStage::Prepare,
    );
    assert!(prepare_hints
        .iter()
        .any(|hint| hint.contains("CHANGELOG.md")));
    assert!(prepare_hints
        .iter()
        .any(|hint| hint.contains("effigy release gates")));

    let execute_hints = remediation_hints_for_blockers(
        &[
            "release state is stale; rerun `effigy release prepare` or pass `--allow-stale` to acknowledge and continue".to_owned(),
            "working tree contains 1 unexpected change(s)".to_owned(),
        ],
        ReleaseBlockedStage::Execute,
    );
    assert!(execute_hints
        .iter()
        .any(|hint| hint.contains("--allow-stale")));
    assert!(execute_hints
        .iter()
        .any(|hint| hint.contains("only prepared release files remain")));
}

#[test]
fn normalize_verify_install_repo_url_rewrites_scp_style_ssh_remotes() {
    assert_eq!(
        normalize_verify_install_repo_url("git@github.com:betterthanclay/effigy.git"),
        "ssh://git@github.com/betterthanclay/effigy.git"
    );
    assert_eq!(
        normalize_verify_install_repo_url("github.com:betterthanclay/effigy.git"),
        "ssh://github.com/betterthanclay/effigy.git"
    );
}

#[test]
fn normalize_verify_install_repo_url_keeps_supported_non_ssh_forms() {
    assert_eq!(
        normalize_verify_install_repo_url("https://github.com/betterthanclay/effigy.git"),
        "https://github.com/betterthanclay/effigy.git"
    );
    assert_eq!(
        normalize_verify_install_repo_url("file:///tmp/effigy.git"),
        "file:///tmp/effigy.git"
    );
    assert_eq!(normalize_verify_install_repo_url("../effigy"), "../effigy");
    assert_eq!(
        normalize_verify_install_repo_url("localhost:8080"),
        "localhost:8080"
    );
}

#[test]
fn resolve_verify_install_repo_url_normalizes_origin_ssh_remote() {
    let root = std::env::temp_dir().join(format!(
        "effigy-release-verify-install-remote-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("mkdir");

    let init = std::process::Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["init", "--quiet"])
        .status()
        .expect("git init");
    assert!(init.success(), "git init should succeed");

    let remote = std::process::Command::new("git")
        .arg("-C")
        .arg(&root)
        .args([
            "remote",
            "add",
            "origin",
            "git@github.com:betterthanclay/effigy.git",
        ])
        .status()
        .expect("git remote add");
    assert!(remote.success(), "git remote add should succeed");

    let resolved = ResolvedTarget {
        resolved_root: root,
        resolution_mode: ResolutionMode::Explicit,
        evidence: vec!["test fixture".to_owned()],
        warnings: Vec::new(),
    };

    let repo_url = resolve_verify_install_repo_url(&resolved, None).expect("resolve repo remote");
    assert_eq!(repo_url, "ssh://git@github.com/betterthanclay/effigy.git");
}

#[test]
fn review_menu_renderers_show_review_markers() {
    let prepare_lines = render_prepare_review_menu_lines(
        &ReleasePreparePlan {
            repo_root: std::env::temp_dir(),
            current_version: semver::Version::new(0, 2, 4),
            version_source: ResolvedVersionSource {
                path: std::env::temp_dir().join("Cargo.toml"),
                kind: VersionFileKind::CargoToml,
                field_path: Some("package.version".to_owned()),
            },
            suggested_version: Some(semver::Version::new(0, 2, 5)),
            planned_version: Some(semver::Version::new(0, 2, 5)),
            suggested_tag: Some("v0.2.5".to_owned()),
            tag: Some("v0.2.5".to_owned()),
            version_override_used: false,
            release_date: "2026-03-11".to_owned(),
            gates_checked: true,
            configured_gate_count: 1,
            gate_results: Vec::new(),
            blockers: Vec::new(),
            mutations: Vec::new(),
            ready: true,
        },
        true,
        PrepareReviewState {
            version_reviewed: true,
            mutations_reviewed: false,
            gates_reviewed: true,
            final_reviewed: false,
        },
    )
    .join("\n");
    assert!(prepare_lines.contains("Reviewed sections: version=reviewed"));
    assert!(prepare_lines.contains("[2] Mutation Review [pending]"));
    assert!(prepare_lines.contains("[3] Gate Review [reviewed]"));

    let execute_lines = render_execute_review_menu_lines(
        &ReleaseExecutePlan {
            repo_root: std::env::temp_dir(),
            state_file: std::env::temp_dir().join(".release-prepared.json"),
            previous_version: Some(semver::Version::new(0, 2, 4)),
            suggested_version: Some(semver::Version::new(0, 2, 5)),
            prepared_version: Some(semver::Version::new(0, 2, 5)),
            suggested_tag: Some("v0.2.5".to_owned()),
            tag: Some("v0.2.5".to_owned()),
            version_override_used: false,
            release_date: Some("2026-03-11".to_owned()),
            prepared_at: Some("2026-03-11T14:00:00+00:00".to_owned()),
            state_loaded: true,
            gates_checked: true,
            gates_passed: true,
            stale: true,
            stale_threshold_seconds: 3600,
            stale_override_required: true,
            stale_override_used: false,
            prepared_branch: Some("main".to_owned()),
            prepared_head: Some("abc123".to_owned()),
            branch: Some("main".to_owned()),
            current_head: Some("abc123".to_owned()),
            remote: Some("origin".to_owned()),
            expected_files: vec!["Cargo.toml".to_owned()],
            modified_files: vec!["Cargo.toml".to_owned()],
            missing_expected_files: Vec::new(),
            unexpected_files: vec!["notes.txt".to_owned()],
            source_fingerprint_available: true,
            fingerprint_drift: Vec::new(),
            warnings: vec!["stale state".to_owned()],
            blockers: vec!["working tree contains 1 unexpected change(s)".to_owned()],
            ready: false,
        },
        false,
        ExecuteReviewState {
            stale_reviewed: true,
            state_reviewed: true,
            working_tree_reviewed: false,
            final_reviewed: false,
        },
    )
    .join("\n");
    assert!(execute_lines.contains("Reviewed sections: stale=reviewed"));
    assert!(execute_lines.contains("[1] Stale Warning Review [reviewed]"));
    assert!(execute_lines.contains("[3] Working Tree Review [pending]"));
}

#[test]
fn current_repo_release_config_matches_self_hosting_release_surfaces() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let config = load_release_config(root).expect("load release config");

    assert_eq!(config.version_source.path, root.join("Cargo.toml"));
    assert_eq!(
        config.version_source.field_path.as_deref(),
        Some("workspace.package.version")
    );
    assert_eq!(config.changelog_path, root.join("CHANGELOG.md"));
    assert_eq!(config.tag_format, "v{version}");
    assert_eq!(config.sync_files.len(), 1);
    assert_eq!(config.sync_files[0].path, root.join("Cargo.lock"));
    assert_eq!(config.sync_files[0].kind, SyncFileKind::CargoLock);

    let gate_pairs = config
        .gates
        .iter()
        .map(|gate| (gate.name.as_str(), gate.command.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        gate_pairs,
        vec![
            ("ci", "sh scripts/check-release-ci.sh"),
            ("format", "cargo fmt --all -- --check"),
            ("test", "cargo test --workspace"),
            (
                "qa",
                "cargo build --bin effigy && ./target/debug/effigy qa:ci"
            ),
            ("build", "cargo build --release --bin effigy"),
            (
                "smoke",
                "cargo build --bin effigy && ./target/debug/effigy smoke:release"
            ),
            (
                "metadata",
                "cargo build --bin effigy && ./target/debug/effigy deliver release validate"
            ),
        ]
    );

    let manifest_source =
        std::fs::read_to_string(root.join("effigy.toml")).expect("read effigy manifest");
    assert!(manifest_source.contains("config/release.toml"));

    let release_manifest =
        std::fs::read_to_string(root.join("config/release.toml")).expect("read release manifest");
    assert!(release_manifest.contains("sync-files = [\"Cargo.lock\"]"));
    assert!(release_manifest.contains("ci = \"sh scripts/check-release-ci.sh\""));
    assert!(!root.join("scripts/check-release-gates.sh").exists());
    assert!(!root
        .join("scripts/check-release-install-from-tag.sh")
        .exists());
    assert!(!root.join("scripts/check-release-smoke.sh").exists());
    assert!(!root.join("scripts/prepare-release.sh").exists());
}

#[cfg(unix)]
#[test]
fn release_ci_gate_requires_a_successful_manual_run_for_the_exact_head() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let candidate_sha = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .expect("read candidate SHA");
    assert!(candidate_sha.status.success());
    let candidate_sha = String::from_utf8(candidate_sha.stdout)
        .expect("UTF-8 SHA")
        .trim()
        .to_owned();

    let fake_bin =
        std::env::temp_dir().join(format!("effigy-release-ci-gate-{}", std::process::id()));
    std::fs::create_dir_all(&fake_bin).expect("create fake bin");
    let fake_gh = fake_bin.join("gh");
    std::fs::write(
        &fake_gh,
        "#!/bin/sh\nprintf '%s\\n' \"$EFFIGY_TEST_CI_SHA\"\n",
    )
    .expect("write fake gh");
    let mut permissions = std::fs::metadata(&fake_gh)
        .expect("fake gh metadata")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&fake_gh, permissions).expect("make fake gh executable");

    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let run = |reported_sha: &str| {
        std::process::Command::new("sh")
            .arg("scripts/check-release-ci.sh")
            .current_dir(root)
            .env("PATH", &path)
            .env("EFFIGY_TEST_CI_SHA", reported_sha)
            .output()
            .expect("run CI gate")
    };

    let success = run(&candidate_sha);
    assert!(success.status.success());
    assert!(String::from_utf8_lossy(&success.stdout).contains(&candidate_sha));

    let failure = run("different-commit");
    assert!(!failure.status.success());
    let stderr = String::from_utf8_lossy(&failure.stderr);
    assert!(stderr.contains("CI is not green for candidate commit"));
    assert!(stderr.contains("dispatch ci.yml on main for this exact commit"));

    std::fs::remove_dir_all(fake_bin).expect("remove fake bin");
}
