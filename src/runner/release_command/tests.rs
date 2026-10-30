use effigy_release::{
    build_diff_preview, detect_pyproject_version_path, json_value_at_path, plan_release_pipeline,
    render_changelog_preview_line as changelog_preview_line, render_prepared_changelog_contents,
    render_updated_version_contents, replace_json_string_at_path_preserving_layout, suggested_bump,
    toml_value_at_path, BumpKind, GateStage, IdentityKind, ResolvedVersionSource, TargetKind,
    VersionFileKind,
};

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
fn current_repo_release_config_matches_the_contract_054_shape() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let pipeline = plan_release_pipeline(root).expect("plan this repository's release");

    assert_eq!(pipeline.identities.len(), 1, "one semver identity");
    let identity = &pipeline.identities[0];
    assert_eq!(identity.name, "effigy");
    assert_eq!(identity.kind, IdentityKind::Semver);
    assert_eq!(identity.tag_format.as_deref(), Some("v{version}"));

    let release_manifest =
        std::fs::read_to_string(root.join("config/release.toml")).expect("read release manifest");
    assert!(release_manifest.contains("sync-files = [\"Cargo.lock\"]"));
    assert!(release_manifest.contains("version-path = \"workspace.package.version\""));
    assert!(!release_manifest.contains("[distribution]"));

    let names = pipeline
        .gates
        .iter()
        .map(|gate| gate.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "preflight",
            "secret-audit",
            "ci",
            "format",
            "test",
            "qa",
            "build",
            "smoke",
            "metadata"
        ]
    );
    let preflight = pipeline
        .gates
        .iter()
        .find(|gate| gate.name == "preflight")
        .expect("preflight gate");
    assert_eq!(preflight.stage, GateStage::Pre);
    assert_eq!(
        preflight.command,
        "sh scripts/release/checks.sh preflight --skip-smoke"
    );
    assert_eq!(preflight.task, None);
    let audit = pipeline
        .gates
        .iter()
        .find(|gate| gate.name == "secret-audit")
        .expect("secret audit gate");
    assert_eq!(audit.stage, GateStage::Certify);
    assert_eq!(
        audit.command,
        "sh scripts/release/secret-audit.sh --base v0.14.1 --base-sha 20465f1ff690340ee9ef461915ca769da1c76fca"
    );
    let metadata = pipeline
        .gates
        .iter()
        .find(|gate| gate.name == "metadata")
        .expect("metadata gate");
    assert_eq!(metadata.stage, GateStage::Certify);
    assert_eq!(metadata.task.as_deref(), Some("release:metadata"));
    for gate in &pipeline.gates {
        if gate.name != "preflight" && gate.name != "metadata" {
            assert_eq!(gate.stage, GateStage::Certify, "{}", gate.name);
        }
    }

    assert_eq!(pipeline.artifacts.len(), 1, "one certified artifact");
    assert_eq!(pipeline.artifacts[0].name, "binary");
    assert_eq!(pipeline.artifacts[0].task, "release:artifact");
    assert_eq!(pipeline.artifacts[0].path, "target/release/effigy");

    let targets = pipeline
        .targets
        .iter()
        .map(|target| (target.name.as_str(), target.kind))
        .collect::<Vec<_>>();
    assert_eq!(
        targets,
        vec![("tag", TargetKind::Tag), ("binaries", TargetKind::Task),]
    );
    let binaries = &pipeline.targets[1];
    assert_eq!(binaries.task.as_deref(), Some("release:publish:binaries"));
    assert_eq!(binaries.check.as_deref(), Some("release:check:binaries"));
    assert_eq!(binaries.verify.as_deref(), Some("release:verify:binaries"));

    // A non-`tag` target makes `publish` an approval checkpoint by default
    // (contract 054 §10 ruling 3).
    assert_eq!(pipeline.approvals, vec!["publish".to_owned()]);
    assert!(pipeline.approval_defaulted);

    let manifest_source =
        std::fs::read_to_string(root.join("effigy.toml")).expect("read effigy manifest");
    assert!(manifest_source.contains("config/release.toml"));
    assert!(!root.join("scripts/check-release-gates.sh").exists());
    assert!(!root
        .join("scripts/check-release-install-from-tag.sh")
        .exists());
    assert!(!root.join("scripts/check-release-smoke.sh").exists());
    assert!(!root.join("scripts/prepare-release.sh").exists());
    assert!(root.join("scripts/release/checks.sh").is_file());
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
    // The gate reads `gh run list --json ... --jq '<one line per run>'`, so the
    // fake answers with one run row: id, head SHA, status, conclusion, url. The
    // head SHA is the answer under test: a run for another commit is a sibling
    // dispatch, not this candidate's.
    std::fs::write(
        &fake_gh,
        "#!/bin/sh\nprintf '%s\\n' \"4242 $EFFIGY_TEST_CI_SHA completed success https://example.invalid/actions/runs/4242\"\n",
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
    assert!(
        stderr.contains("hosted CI run #4242 is for commit different-commit"),
        "{stderr}"
    );
    assert!(stderr.contains(&candidate_sha), "{stderr}");

    std::fs::remove_dir_all(fake_bin).expect("remove fake bin");
}

#[test]
fn release_gate_steps_name_the_gate_and_its_outcome() {
    use super::ops::{gate_progress, release_gate_summary};
    use effigy_release::{GateResult, ResolvedGate};
    use std::time::Duration;

    let gate = ResolvedGate {
        name: "cargo test".to_owned(),
        command: "effigy test".to_owned(),
        task: Some("test".to_owned()),
        description: None,
        stage: effigy_release::GateStage::Certify,
        reuse_hosted_evidence: false,
        hosted_evidence: None,
    };
    assert_eq!(
        gate_progress(2, 5, &gate, false).frame_at(Duration::from_secs(3)),
        "running gate cargo test (2/5); waiting for `effigy test` to exit; 3s elapsed"
    );
    let reused = ResolvedGate {
        name: "hosted ci".to_owned(),
        reuse_hosted_evidence: true,
        ..gate
    };
    assert_eq!(
        gate_progress(1, 1, &reused, true).frame_at(Duration::from_secs(2)),
        "reusing hosted evidence for gate hosted ci (1/1); waiting for hosted evidence for gate hosted ci; 2s elapsed"
    );

    let failed = GateResult {
        name: "cargo test".to_owned(),
        description: None,
        command: "effigy test".to_owned(),
        passed: false,
        exit_code: Some(101),
        stdout: String::new(),
        stderr: "a test failed".to_owned(),
        launch_error: None,
        duration_ms: 12_000,
        log_path: Some(std::path::PathBuf::from(
            ".effigy/release/gates/cargo-test.log",
        )),
        reused: false,
        hosted_run_url: None,
        hosted_head_sha: None,
        hosted_repository: None,
    };
    let summary = release_gate_summary(2, 5, &failed);
    assert!(
        summary.contains("gate `cargo test` failed after 12s"),
        "{summary}"
    );
    assert!(summary.contains("(2/5)"), "{summary}");
    assert!(summary.contains("its output is in"), "{summary}");
}

#[test]
fn a_release_run_step_names_the_stage_the_gate_and_the_next_stage() {
    use super::ops::{ReleaseRunSteps, NEXT_GATE_OR_STAGE};
    use effigy_release::{GateObserver, GateResult, ResolvedGate};
    use effigy_ui::steps::ProgressSource;
    use std::time::Duration;

    // A test process draws nothing, so the frames are read from the source on a
    // fake clock.
    let progress = ProgressSource::waiting_for(NEXT_GATE_OR_STAGE);
    progress.update("running the release through publish", None, None);
    let root = effigy_ui::steps::live_step(progress.clone());
    let mut observer = ReleaseRunSteps {
        root: root.clone(),
        progress: progress.clone(),
        gate: None,
    };

    observer.stage("certify");
    assert_eq!(
        progress.frame_at(Duration::from_secs(3)),
        "running stage certify; waiting for the next gate or stage; 3s elapsed"
    );

    let gate = ResolvedGate {
        name: "cargo test".to_owned(),
        command: "effigy test".to_owned(),
        task: Some("test".to_owned()),
        description: None,
        stage: effigy_release::GateStage::Certify,
        reuse_hosted_evidence: false,
        hosted_evidence: None,
    };
    observer.gate_started(2, 5, &gate, false);
    assert_eq!(
        progress.frame_at(Duration::from_secs(4)),
        "running gate cargo test (2/5); waiting for `effigy test` to exit; 4s elapsed"
    );

    let result = GateResult {
        name: "cargo test".to_owned(),
        description: None,
        command: "effigy test".to_owned(),
        passed: true,
        exit_code: Some(0),
        stdout: String::new(),
        stderr: String::new(),
        launch_error: None,
        duration_ms: 12_000,
        log_path: None,
        reused: false,
        hosted_run_url: None,
        hosted_head_sha: None,
        hosted_repository: None,
    };
    observer.gate_finished(2, 5, &result);
    assert_eq!(
        progress.frame_at(Duration::from_secs(40)),
        "checking the next gate or stage (2/5); last: gate `cargo test` passed after 12s (2/5); 40s elapsed"
    );

    // A wait that is not one gate names itself as the sub-step; the last gate
    // outcome stays visible until a newer event replaces it.
    observer.note("checking hosted GitHub Actions evidence");
    assert_eq!(
        progress.frame_at(Duration::from_secs(41)),
        "checking hosted GitHub Actions evidence; last: gate `cargo test` passed after 12s (2/5); 41s elapsed"
    );
}

#[test]
fn a_paused_release_run_names_what_it_waits_on() {
    use super::ops::release_run_summary_text;

    assert_eq!(
        release_run_summary_text("publish", "awaiting approval", Some("publish")),
        "release reached stage `publish` (awaiting approval); waiting on `publish`"
    );
    assert_eq!(
        release_run_summary_text("certify", "complete", None),
        "release reached stage `certify` (complete)"
    );
}
