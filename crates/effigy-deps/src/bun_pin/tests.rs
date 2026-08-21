use std::cell::RefCell;
use std::fs;

use tempfile::TempDir;

use super::*;
use crate::{
    ConsumerRoot, DependencyLinkKey, DependencyPackage, DesiredDependencyLink, LinkMechanism,
    PackageManager, ProcessOutput, ProcessRequest, RepoLinkState, RepoLinkStateStore,
};

struct FixtureProcess {
    stdout: String,
    requests: RefCell<Vec<ProcessRequest>>,
}

impl FixtureProcess {
    fn new(stdout: &str) -> Self {
        Self {
            stdout: stdout.to_owned(),
            requests: RefCell::new(Vec::new()),
        }
    }
}

impl ReadOnlyProcess for FixtureProcess {
    fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, DepsError> {
        self.requests.borrow_mut().push(request.clone());
        Ok(ProcessOutput {
            status: Some(0),
            stdout: self.stdout.clone(),
            stderr: String::new(),
        })
    }
}

struct FailingProcess {
    requests: RefCell<Vec<ProcessRequest>>,
}

impl FailingProcess {
    fn new() -> Self {
        Self {
            requests: RefCell::new(Vec::new()),
        }
    }
}

struct MissingProcess;

impl ReadOnlyProcess for MissingProcess {
    fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, DepsError> {
        Err(DepsError::ProcessSpawn {
            program: request.program.clone(),
            cwd: request.cwd.clone(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "fixture Bun missing"),
        })
    }
}

impl ReadOnlyProcess for FailingProcess {
    fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, DepsError> {
        self.requests.borrow_mut().push(request.clone());
        Err(DepsError::ProcessFailed {
            program: request.program.clone(),
            cwd: request.cwd.clone(),
            status: Some(1),
            stderr: "Error loading lockfile: InvalidPackageInfo".to_owned(),
        })
    }
}

struct Fixture {
    _temp: TempDir,
    consumer: PathBuf,
    library: PathBuf,
}

fn fixture(packages: &[(&str, &str)]) -> Fixture {
    let temp = TempDir::new().unwrap();
    let consumer = temp.path().join("consumer");
    let library = temp.path().join("library");
    write(
        &library.join("package.json"),
        r#"{"private":true,"workspaces":["packages/*"]}"#,
    );
    for (name, directory) in packages {
        write(
            &library
                .join("packages")
                .join(directory)
                .join("package.json"),
            &format!(r#"{{"name":"{name}","version":"1.0.0"}}"#),
        );
    }
    fs::create_dir_all(&consumer).unwrap();
    Fixture {
        _temp: temp,
        consumer,
        library,
    }
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

fn package_tree(names: &[&str]) -> String {
    names
        .iter()
        .map(|name| format!("├── {name}@1.0.0\n"))
        .collect()
}

#[test]
fn pin_plans_the_unique_full_closure_in_package_order() {
    let fixture = fixture(&[("@acme/ui", "ui"), ("@acme/core", "core")]);
    write(
        &fixture.consumer.join("package.json"),
        r#"{"name":"consumer","dependencies":{"@acme/ui":"^1"}}"#,
    );
    let process = FixtureProcess::new(
        "consumer node_modules\n├── @acme/ui@1.0.0\n│  └── @acme/core@1.0.0\n└── @acme/core@2.0.0\n",
    );

    let plan = plan_bun_pin(&fixture.consumer, &fixture.library, true, &process).unwrap();

    assert_eq!(plan.disposition, BunPinPlanDisposition::Apply);
    assert_eq!(
        plan.packages
            .iter()
            .map(|package| package.name.as_str())
            .collect::<Vec<_>>(),
        ["@acme/core", "@acme/ui"]
    );
    assert_eq!(plan.packages[0].depth, Some(DependencyDepth::Transitive));
    assert_eq!(plan.packages[1].depth, Some(DependencyDepth::Direct));
    assert_eq!(process.requests.borrow().len(), 1);
    assert_eq!(process.requests.borrow()[0].args, ["pm", "ls", "--all"]);
}

#[test]
fn pin_falls_back_to_jsonc_lock_packages_after_bun_enumeration_fails() {
    let fixture = fixture(&[("@acme/ui", "ui"), ("@acme/core", "core")]);
    let manifest = r#"{"name":"consumer","dependencies":{"@acme/ui":"^1"}}"#;
    write(&fixture.consumer.join("package.json"), manifest);
    write(
        &fixture.consumer.join("bun.lock"),
        concat!(
            "{\n",
            "  // Bun text locks are JSONC.\n",
            "  \"lockfileVersion\": 1,\n",
            "  \"packages\": {\n",
            "    \"@acme/ui\": [\"@acme/ui@1.0.0\", {}, \"sha512-ui\"],\n",
            "    \"host/@acme/core\": [\"@acme/core@file:../cache/core\", {}, \"sha512-core\"],\n",
            "  },\n",
            "}\n",
        ),
    );
    let process = FailingProcess::new();

    let plan = plan_bun_pin(&fixture.consumer, &fixture.library, true, &process).unwrap();

    assert_eq!(plan.disposition, BunPinPlanDisposition::Apply);
    assert_eq!(
        plan.packages
            .iter()
            .map(|package| (package.name.as_str(), package.depth))
            .collect::<Vec<_>>(),
        [
            ("@acme/core", Some(DependencyDepth::Transitive)),
            ("@acme/ui", Some(DependencyDepth::Direct)),
        ]
    );
    let warning = plan
        .warnings
        .iter()
        .find(|warning| warning.code == "lockfile-enumeration-fallback")
        .unwrap();
    assert!(warning.message.contains("InvalidPackageInfo"));
    assert!(warning.message.contains("bun.lock"));
    assert_eq!(process.requests.borrow().len(), 1);
    assert_eq!(
        fs::read_to_string(fixture.consumer.join("package.json")).unwrap(),
        manifest
    );
}

#[test]
fn pin_falls_back_to_jsonc_lock_when_bun_is_unavailable() {
    let fixture = fixture(&[("@acme/ui", "ui")]);
    write(
        &fixture.consumer.join("package.json"),
        r#"{"name":"consumer","dependencies":{"@acme/ui":"^1"}}"#,
    );
    write(
        &fixture.consumer.join("bun.lock"),
        r#"{"packages":{"@acme/ui":["@acme/ui@1.0.0",{},""]}}"#,
    );

    let plan = plan_bun_pin(&fixture.consumer, &fixture.library, true, &MissingProcess).unwrap();

    assert_eq!(plan.disposition, BunPinPlanDisposition::Apply);
    assert_eq!(plan.packages[0].name, "@acme/ui");
    let warning = plan
        .warnings
        .iter()
        .find(|warning| warning.code == "lockfile-enumeration-fallback")
        .unwrap();
    assert!(warning.message.contains("fixture Bun missing"));
    assert!(warning.message.contains("bun.lock"));
}

#[test]
fn pin_lock_fallback_refuses_missing_malformed_and_unsafe_lock_data() {
    let fixture = fixture(&[("@acme/core", "core")]);
    let manifest = r#"{"name":"consumer","dependencies":{"@acme/core":"^1"}}"#;
    write(&fixture.consumer.join("package.json"), manifest);

    let missing = plan_bun_pin(
        &fixture.consumer,
        &fixture.library,
        false,
        &FailingProcess::new(),
    )
    .unwrap_err();
    assert!(missing
        .to_string()
        .contains("text lockfile fallback failed"));

    write(&fixture.consumer.join("bun.lock"), "{ not-jsonc");
    let malformed = plan_bun_pin(
        &fixture.consumer,
        &fixture.library,
        false,
        &FailingProcess::new(),
    )
    .unwrap_err();
    assert!(malformed.to_string().contains("failed to parse"));

    write(
        &fixture.consumer.join("bun.lock"),
        r#"{"packages":{"@acme/core":{"specifier":"@acme/core@1.0.0"}}}"#,
    );
    let unsafe_record = plan_bun_pin(
        &fixture.consumer,
        &fixture.library,
        false,
        &FailingProcess::new(),
    )
    .unwrap_err();
    assert!(unsafe_record.to_string().contains("must be an array"));
    assert!(unsafe_record.to_string().contains("InvalidPackageInfo"));
    assert_eq!(
        fs::read_to_string(fixture.consumer.join("package.json")).unwrap(),
        manifest
    );
}

#[test]
fn shared_bun_inventory_does_not_use_the_pin_lockfile_fallback() {
    let fixture = fixture(&[("@acme/core", "core")]);
    write(
        &fixture.consumer.join("package.json"),
        r#"{"dependencies":{"@acme/core":"^1"}}"#,
    );
    write(
        &fixture.consumer.join("bun.lock"),
        r#"{"packages":{"@acme/core":["@acme/core@1.0.0",{},""]}}"#,
    );
    let packages = inventory_bun_library(&fixture.library).unwrap();

    let error =
        inventory_bun_consumer(&fixture.consumer, &packages, &FailingProcess::new()).unwrap_err();

    assert!(matches!(error, DepsError::ProcessFailed { .. }));
}

#[test]
fn pin_applies_one_layout_preserving_manifest_edit_and_keeps_locks_exact() {
    let fixture = fixture(&[("@acme/core", "core"), ("@acme/ui", "ui")]);
    let before = concat!(
        "{\n",
        "    \"name\": \"consumer\",\n",
        "    \"scripts\": { \"check\": \"keep spacing\" },\n",
        "    \"overrides\": {\n",
        "        \"unrelated\": \"^2\"\n",
        "    },\n",
        "    \"private\": true\n",
        "}\n"
    );
    write(&fixture.consumer.join("package.json"), before);
    write(&fixture.consumer.join("bun.lock"), "text-lock\n");
    fs::write(fixture.consumer.join("bun.lockb"), [0, 1, 2, 3]).unwrap();
    let process = FixtureProcess::new(&package_tree(&["@acme/ui", "@acme/core"]));

    let plan = plan_bun_pin(&fixture.consumer, &fixture.library, false, &process).unwrap();
    assert_eq!(plan.warnings.len(), 2);
    let report = apply_bun_pin_plan(plan);

    assert_eq!(report.outcome, BunPinOutcome::Applied);
    assert_eq!(report.writes.len(), 1);
    assert_eq!(
        report.verification.status,
        BunPinVerificationStatus::ManifestVerified
    );
    assert!(report.verification.install_pending);
    assert!(report
        .verification
        .immutable_files
        .iter()
        .all(|file| file.unchanged));
    assert_eq!(
        fs::read_to_string(fixture.consumer.join("bun.lock")).unwrap(),
        "text-lock\n"
    );
    assert_eq!(
        fs::read(fixture.consumer.join("bun.lockb")).unwrap(),
        [0, 1, 2, 3]
    );
    assert_eq!(
        fs::read_to_string(fixture.consumer.join("package.json")).unwrap(),
        concat!(
            "{\n",
            "    \"name\": \"consumer\",\n",
            "    \"scripts\": { \"check\": \"keep spacing\" },\n",
            "    \"overrides\": {\n",
            "        \"unrelated\": \"^2\",\n",
            "        \"@acme/core\": \"file:../library/packages/core\",\n",
            "        \"@acme/ui\": \"file:../library/packages/ui\"\n",
            "    },\n",
            "    \"private\": true\n",
            "}\n"
        )
    );
}

#[test]
fn pin_creates_missing_overrides_with_existing_tabs_and_newline_posture() {
    let fixture = fixture(&[("@acme/core", "core")]);
    let before = "{\r\n\t\"name\": \"consumer\",\r\n\t\"private\": true\r\n}";
    write(&fixture.consumer.join("package.json"), before);
    let process = FixtureProcess::new(&package_tree(&["@acme/core"]));

    let report = apply_bun_pin_plan(
        plan_bun_pin(&fixture.consumer, &fixture.library, false, &process).unwrap(),
    );

    assert_eq!(report.outcome, BunPinOutcome::Applied);
    assert_eq!(
        fs::read_to_string(fixture.consumer.join("package.json")).unwrap(),
        concat!(
            "{\r\n",
            "\t\"name\": \"consumer\",\r\n",
            "\t\"private\": true,\r\n",
            "\t\"overrides\": {\r\n",
            "\t\t\"@acme/core\": \"file:../library/packages/core\"\r\n",
            "\t}\r\n",
            "}"
        )
    );
}

#[test]
fn pin_keeps_an_inline_manifest_inline_before_its_final_newline() {
    let fixture = fixture(&[("@acme/core", "core")]);
    write(
        &fixture.consumer.join("package.json"),
        "{\"name\": \"consumer\"}\n",
    );
    let process = FixtureProcess::new(&package_tree(&["@acme/core"]));

    let report = apply_bun_pin_plan(
        plan_bun_pin(&fixture.consumer, &fixture.library, false, &process).unwrap(),
    );

    assert_eq!(report.outcome, BunPinOutcome::Applied);
    assert_eq!(
        fs::read_to_string(fixture.consumer.join("package.json")).unwrap(),
        "{\"name\": \"consumer\", \"overrides\": {\"@acme/core\": \"file:../library/packages/core\"}}\n"
    );
}

#[test]
fn exact_repin_is_already_applied_and_does_not_write() {
    let fixture = fixture(&[("@acme/core", "core")]);
    let before = r#"{"overrides":{"@acme/core":"file:../library/packages/core"}}"#;
    write(&fixture.consumer.join("package.json"), before);
    let process = FixtureProcess::new(&package_tree(&["@acme/core"]));

    let plan = plan_bun_pin(&fixture.consumer, &fixture.library, false, &process).unwrap();
    assert_eq!(plan.disposition, BunPinPlanDisposition::AlreadyApplied);
    let report = apply_bun_pin_plan(plan);

    assert_eq!(report.outcome, BunPinOutcome::AlreadyApplied);
    assert!(report.writes.is_empty());
    assert_eq!(
        fs::read_to_string(fixture.consumer.join("package.json")).unwrap(),
        before
    );
}

#[test]
fn one_conflict_blocks_the_complete_pin_plan() {
    let fixture = fixture(&[("@acme/core", "core"), ("@acme/ui", "ui")]);
    let elsewhere = fixture._temp.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    let before = r#"{"overrides":{"@acme/core":"file:../elsewhere"}}"#;
    write(&fixture.consumer.join("package.json"), before);
    let process = FixtureProcess::new(&package_tree(&["@acme/core", "@acme/ui"]));

    let plan = plan_bun_pin(&fixture.consumer, &fixture.library, false, &process).unwrap();
    assert_eq!(plan.disposition, BunPinPlanDisposition::Conflict);
    assert!(plan
        .packages
        .iter()
        .all(|package| package.action == BunPinPackageAction::Conflict));
    let report = apply_bun_pin_plan(plan);

    assert_eq!(report.outcome, BunPinOutcome::Conflict);
    assert!(!report.outcome.is_success());
    assert!(report.writes.is_empty());
    assert_eq!(
        fs::read_to_string(fixture.consumer.join("package.json")).unwrap(),
        before
    );
}

#[test]
fn unpin_removes_only_exact_library_entries_and_preserves_formatting() {
    let fixture = fixture(&[("@acme/core", "core"), ("@acme/ui", "ui")]);
    let elsewhere = fixture._temp.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    write(
        &fixture.consumer.join("package.json"),
        concat!(
            "{\n",
            "  \"name\": \"consumer\",\n",
            "  \"overrides\": {\n",
            "    \"@acme/core\": \"file:../library/packages/core\",\n",
            "    \"@acme/ui\": \"file:../elsewhere\",\n",
            "    \"unrelated\": \"^2\"\n",
            "  },\n",
            "  \"private\": true\n",
            "}\n"
        ),
    );

    let plan = plan_bun_unpin(&fixture.consumer, &fixture.library, false).unwrap();
    assert_eq!(plan.disposition, BunPinPlanDisposition::Apply);
    assert_eq!(plan.packages[0].action, BunPinPackageAction::Remove);
    assert_eq!(plan.packages[1].action, BunPinPackageAction::AlreadyApplied);
    let report = apply_bun_pin_plan(plan);

    assert_eq!(report.outcome, BunPinOutcome::Applied);
    assert_eq!(
        fs::read_to_string(fixture.consumer.join("package.json")).unwrap(),
        concat!(
            "{\n",
            "  \"name\": \"consumer\",\n",
            "  \"overrides\": {\n",
            "    \"@acme/ui\": \"file:../elsewhere\",\n",
            "    \"unrelated\": \"^2\"\n",
            "  },\n",
            "  \"private\": true\n",
            "}\n"
        )
    );
}

#[test]
fn unpin_removes_interleaved_first_middle_and_last_entries() {
    let fixture = fixture(&[("@acme/a", "a"), ("@acme/c", "c"), ("@acme/e", "e")]);
    write(
        &fixture.consumer.join("package.json"),
        concat!(
            "{\n",
            "  \"overrides\": {\n",
            "    \"@acme/a\": \"file:../library/packages/a\",\n",
            "    \"keep-b\": \"^1\",\n",
            "    \"@acme/c\": \"file:../library/packages/c\",\n",
            "    \"keep-d\": \"^1\",\n",
            "    \"@acme/e\": \"file:../library/packages/e\"\n",
            "  }\n",
            "}\n"
        ),
    );

    let report =
        apply_bun_pin_plan(plan_bun_unpin(&fixture.consumer, &fixture.library, false).unwrap());

    assert_eq!(report.outcome, BunPinOutcome::Applied);
    assert_eq!(
        fs::read_to_string(fixture.consumer.join("package.json")).unwrap(),
        concat!(
            "{\n",
            "  \"overrides\": {\n",
            "    \"keep-b\": \"^1\",\n",
            "    \"keep-d\": \"^1\"\n",
            "  }\n",
            "}\n"
        )
    );
}

#[test]
fn unpin_removes_the_empty_overrides_property_only() {
    let fixture = fixture(&[("@acme/core", "core")]);
    write(
        &fixture.consumer.join("package.json"),
        concat!(
            "{\n",
            "  \"name\": \"consumer\",\n",
            "  \"overrides\": {\n",
            "    \"@acme/core\": \"file:../library/packages/core\"\n",
            "  },\n",
            "  \"private\": true\n",
            "}\n"
        ),
    );

    let report =
        apply_bun_pin_plan(plan_bun_unpin(&fixture.consumer, &fixture.library, false).unwrap());

    assert_eq!(report.outcome, BunPinOutcome::Applied);
    assert_eq!(
        fs::read_to_string(fixture.consumer.join("package.json")).unwrap(),
        concat!(
            "{\n",
            "  \"name\": \"consumer\",\n",
            "  \"private\": true\n",
            "}\n"
        )
    );
}

#[test]
fn stale_manifest_refuses_apply_without_overwriting_the_new_content() {
    let fixture = fixture(&[("@acme/core", "core")]);
    write(
        &fixture.consumer.join("package.json"),
        "{\n  \"name\": \"consumer\"\n}\n",
    );
    let process = FixtureProcess::new(&package_tree(&["@acme/core"]));
    let plan = plan_bun_pin(&fixture.consumer, &fixture.library, false, &process).unwrap();
    let changed = "{\n  \"name\": \"consumer\",\n  \"private\": true\n}\n";
    write(&fixture.consumer.join("package.json"), changed);

    let report = apply_bun_pin_plan(plan);

    assert_eq!(report.outcome, BunPinOutcome::ApplyFailed);
    assert!(report.writes.is_empty());
    assert_eq!(
        fs::read_to_string(fixture.consumer.join("package.json")).unwrap(),
        changed
    );
}

struct FailingWriter;

impl ManifestWriter for FailingWriter {
    fn write(&self, path: &Path, _contents: &[u8]) -> Result<(), DepsError> {
        Err(DepsError::io(
            "replace package manifest",
            path,
            std::io::Error::other("injected failure"),
        ))
    }
}

#[test]
fn write_failure_leaves_the_original_manifest_intact() {
    let fixture = fixture(&[("@acme/core", "core")]);
    let before = "{\n  \"name\": \"consumer\"\n}\n";
    write(&fixture.consumer.join("package.json"), before);
    let process = FixtureProcess::new(&package_tree(&["@acme/core"]));
    let plan = plan_bun_pin(&fixture.consumer, &fixture.library, false, &process).unwrap();

    let report = apply_with_writer(plan, &FailingWriter);

    assert_eq!(report.outcome, BunPinOutcome::ApplyFailed);
    assert!(report.writes.is_empty());
    assert_eq!(
        fs::read_to_string(fixture.consumer.join("package.json")).unwrap(),
        before
    );
}

#[test]
fn dry_run_and_no_match_never_write() {
    let fixture = fixture(&[("@acme/core", "core")]);
    let before = r#"{"name":"consumer"}"#;
    write(&fixture.consumer.join("package.json"), before);

    let dry_run = apply_bun_pin_plan(
        plan_bun_pin(
            &fixture.consumer,
            &fixture.library,
            true,
            &FixtureProcess::new(&package_tree(&["@acme/core"])),
        )
        .unwrap(),
    );
    assert_eq!(dry_run.outcome, BunPinOutcome::DryRun);
    assert!(dry_run.writes.is_empty());
    assert_eq!(
        fs::read_to_string(fixture.consumer.join("package.json")).unwrap(),
        before
    );

    let no_match = apply_bun_pin_plan(
        plan_bun_pin(
            &fixture.consumer,
            &fixture.library,
            false,
            &FixtureProcess::new("consumer node_modules\n└── unrelated@1.0.0\n"),
        )
        .unwrap(),
    );
    assert_eq!(no_match.outcome, BunPinOutcome::NoMatch);
    assert!(no_match.writes.is_empty());
    assert_eq!(
        fs::read_to_string(fixture.consumer.join("package.json")).unwrap(),
        before
    );
}

#[test]
fn invalid_overrides_shape_is_rejected_during_planning() {
    let fixture = fixture(&[("@acme/core", "core")]);
    write(
        &fixture.consumer.join("package.json"),
        r#"{"overrides":"not-an-object"}"#,
    );

    let error = plan_bun_pin(
        &fixture.consumer,
        &fixture.library,
        false,
        &FixtureProcess::new(&package_tree(&["@acme/core"])),
    )
    .unwrap_err();

    assert!(error.to_string().contains("`overrides` must be an object"));
}

#[test]
fn duplicate_override_keys_are_rejected_even_for_an_apparent_no_op() {
    let fixture = fixture(&[("@acme/core", "core")]);
    write(
        &fixture.consumer.join("package.json"),
        r#"{"overrides":{"@acme/core":"^1","@acme/core":"file:../library/packages/core"}}"#,
    );

    let error = plan_bun_pin(
        &fixture.consumer,
        &fixture.library,
        false,
        &FixtureProcess::new(&package_tree(&["@acme/core"])),
    )
    .unwrap_err();

    assert!(error
        .to_string()
        .contains("duplicate JSON object key `@acme/core`"));
}

#[test]
fn unreadable_immutable_path_never_reports_unchanged() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("bun.lock");
    fs::create_dir(&path).unwrap();

    let evidence = immutable_evidence(&[FileSnapshot {
        path: path.clone(),
        contents: None,
    }]);

    assert_eq!(evidence.len(), 1);
    assert_eq!(evidence[0].path, path);
    assert!(!evidence[0].unchanged);
}

#[test]
fn lockfile_change_after_planning_refuses_manifest_write() {
    let fixture = fixture(&[("@acme/core", "core")]);
    let before = r#"{"name":"consumer"}"#;
    write(&fixture.consumer.join("package.json"), before);
    write(&fixture.consumer.join("bun.lock"), "before\n");
    let process = FixtureProcess::new(&package_tree(&["@acme/core"]));
    let plan = plan_bun_pin(&fixture.consumer, &fixture.library, false, &process).unwrap();
    write(&fixture.consumer.join("bun.lock"), "after\n");

    let report = apply_bun_pin_plan(plan);

    assert_eq!(report.outcome, BunPinOutcome::ApplyFailed);
    assert!(report.writes.is_empty());
    assert_eq!(
        fs::read_to_string(fixture.consumer.join("package.json")).unwrap(),
        before
    );
    assert_eq!(
        fs::read_to_string(fixture.consumer.join("bun.lock")).unwrap(),
        "after\n"
    );
}

#[test]
fn active_effigy_link_refuses_pin_before_consumer_inventory_or_manifest_write() {
    let fixture = fixture(&[("@acme/core", "core")]);
    let manifest = r#"{"name":"consumer","dependencies":{"@acme/core":"^1"}}"#;
    write(&fixture.consumer.join("package.json"), manifest);
    let consumer = fs::canonicalize(&fixture.consumer).unwrap();
    let library = fs::canonicalize(&fixture.library).unwrap();
    let package_path = fs::canonicalize(fixture.library.join("packages/core")).unwrap();
    let mut state = RepoLinkState::empty();
    state.links.push(DesiredDependencyLink {
        key: DependencyLinkKey {
            manager: PackageManager::Bun,
            consumer_repo: consumer.clone(),
            library_path: library.clone(),
        },
        mechanism: LinkMechanism::BunLink,
        consumer_roots: vec![ConsumerRoot {
            canonical_path: consumer.clone(),
        }],
        packages: vec![DependencyPackage {
            name: "@acme/core".to_owned(),
            local_path: package_path,
            committed_sources: Vec::new(),
        }],
        cargo_resolutions: Vec::new(),
        cargo_ownership: None,
    });
    RepoLinkStateStore::for_repo(&consumer)
        .write(&state)
        .unwrap();
    let process = FixtureProcess::new(&package_tree(&["@acme/core"]));

    let report = apply_bun_pin_plan(plan_bun_pin(&consumer, &library, false, &process).unwrap());

    assert_eq!(report.outcome, BunPinOutcome::Conflict);
    assert!(report.writes.is_empty());
    assert!(report.errors[0].contains("effigy deps unlink bun"));
    assert!(process.requests.borrow().is_empty());
    assert_eq!(
        fs::read_to_string(consumer.join("package.json")).unwrap(),
        manifest
    );
}
