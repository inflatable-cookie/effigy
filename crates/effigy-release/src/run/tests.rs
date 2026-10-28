//! Proofs for the checkpointed release Run (contract 054 §3, §5, §7).
//!
//! Every fixture is a whole project root inside its own `mktemp -d`-style
//! temporary directory, with its own bare remote and its own `HOME`. Nothing
//! here touches a real project, GitHub, a registry or the operator's state.
//!
//! A stop at a checkpoint is produced with `--to <stage>`: it leaves exactly
//! the durable state a crash at that boundary leaves — the record and its
//! kernel checkpoint pointer, with the remaining stages unrun — and the next
//! invocation settles the previous kernel Run before continuing. A crash
//! *between* an irreversible effect and its journal record is proven
//! separately, by an ambiguous tag push that really happened and was not
//! recorded.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::kernel::read_release_observation;
use super::*;

// ---------------------------------------------------------------------------
// fixtures
// ---------------------------------------------------------------------------

struct Fixture {
    root: PathBuf,
    home: PathBuf,
    remote: PathBuf,
    _directory: tempfile::TempDir,
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create fixture directory");
    }
    std::fs::write(&path, contents).expect("write fixture file");
}

fn commit_all(root: &Path, message: &str) {
    git(root, &["add", "."]);
    git(root, &["commit", "-m", message]);
}

fn init_repo(root: &Path) {
    git(root, &["init", "-b", "main"]);
    git(root, &["config", "user.email", "release-tests@example.com"]);
    git(root, &["config", "user.name", "Release Tests"]);
    git(root, &["config", "commit.gpgsign", "false"]);
}

/// A whole project root, its bare remote and its private HOME.
fn fixture(label: &str) -> Fixture {
    let directory = tempfile::Builder::new()
        .prefix(&format!("effigy-release-run-{label}-"))
        .tempdir()
        .expect("fixture directory");
    let root = directory.path().join("project");
    let home = directory.path().join("home");
    let remote = directory.path().join("remote.git");
    std::fs::create_dir_all(&root).expect("project");
    std::fs::create_dir_all(&home).expect("home");
    init_repo(&root);
    let status = Command::new("git")
        .args(["init", "--bare"])
        .arg(&remote)
        .status()
        .expect("bare remote");
    assert!(status.success(), "bare remote init");
    git(
        &root,
        &["remote", "add", "origin", &remote.display().to_string()],
    );
    Fixture {
        root,
        home,
        remote,
        _directory: directory,
    }
}

fn commit_and_push(fixture: &Fixture) {
    commit_all(&fixture.root, "fixture");
    git(&fixture.root, &["push", "-u", "origin", "main"]);
}

const CHANGELOG: &str = "# Changelog\n\n## [Unreleased]\n\n### Added\n- A release Run.\n\n## [0.2.4] - 2026-10-01\n\n### Fixed\n- A previous release.\n";

const PACKAGE_CHANGELOG: &str = "# Changelog\n\n## [Unreleased]\n\n### Added\n- A release Run.\n\n## [1.0.0] - 2026-10-01\n\n### Fixed\n- A previous release.\n";

fn single_identity_project(fixture: &Fixture) {
    write(
        &fixture.root,
        "Cargo.toml",
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    );
    write(&fixture.root, "CHANGELOG.md", CHANGELOG);
    write(&fixture.root, "effigy.toml", SINGLE_IDENTITY_MANIFEST);
    write(&fixture.root, ".gitignore", "dist/\n");
    write(&fixture.root, "gates/pre.sh", "#!/bin/sh\necho pre\n");
    write(
        &fixture.root,
        "gates/certify.sh",
        "#!/bin/sh\nset -e\ngrep -q 'version = \"0.2.5\"' Cargo.toml\necho certified\n",
    );
    commit_and_push(fixture);
}

const SINGLE_IDENTITY_MANIFEST: &str = r#"[release]
changelog = "CHANGELOG.md"

[release.identities.app]
version-file = "Cargo.toml"
version-path = "package.version"
tag-format = "v{version}"

[release.gates]
pre = { command = "sh gates/pre.sh", stage = "pre" }
certify-version = "sh gates/certify.sh"

[release.artifacts]
app = { task = "release:pack", path = "dist/app-*.tgz" }

[release.targets.tags]
kind = "tag"
"#;

fn two_identity_project(fixture: &Fixture) {
    write(
        &fixture.root,
        "packages/core/package.json",
        "{\n  \"name\": \"core\",\n  \"version\": \"1.0.0\"\n}\n",
    );
    write(
        &fixture.root,
        "packages/react/package.json",
        "{\n  \"name\": \"react\",\n  \"version\": \"1.0.0\"\n}\n",
    );
    write(&fixture.root, "CHANGELOG.md", PACKAGE_CHANGELOG);
    write(
        &fixture.root,
        "effigy.toml",
        r#"[release]
changelog = "CHANGELOG.md"

[release.identities.core]
version-file = "packages/core/package.json"
tag-format = "core-v{version}"

[release.identities.react]
version-file = "packages/react/package.json"
tag-format = "react-v{version}"
follows = "core"

[release.targets.tags]
kind = "tag"
"#,
    );
    write(&fixture.root, ".gitignore", "dist/\n");
    commit_and_push(fixture);
}

// ---------------------------------------------------------------------------
// the kernel helper the Run re-execs
// ---------------------------------------------------------------------------

/// The kernel helper binary the release Run re-execs for process custody.
///
/// Custody re-execs the binary the startup path names as its sentinel and
/// supervisor (`__kernel-sentinel`, `__kernel-supervise`; contract 052 §5.7),
/// so the tests
/// supply `kernel-custody-fixture` instead of the libtest binary. The selector
/// `test:release:runtime-storage` builds it and exports
/// `EFFIGY_KERNEL_CUSTODY_FIXTURE`; an ordinary `effigy test` run prepares it
/// through `effigy-kernel`'s `tests/fixture_binary.rs`, so it is either beside
/// the test executable's profile directory or, when Oxide uplifts final
/// binaries to the workspace (Oxide #106), in `target/<profile>/`.
fn custody_fixture() -> effigy_kernel::supervise::Executable {
    let executable = std::env::current_exe().expect("test executable");
    let profile_dir = executable
        .parent()
        .and_then(Path::parent)
        .expect("target profile directory");
    let profile = profile_dir
        .file_name()
        .expect("target profile directory name");
    let candidates: Vec<PathBuf> = std::env::var_os("EFFIGY_KERNEL_CUSTODY_FIXTURE")
        .map(PathBuf::from)
        .into_iter()
        .chain([
            profile_dir.join("kernel-custody-fixture"),
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target")
                .join(profile)
                .join("kernel-custody-fixture"),
        ])
        .collect();
    for candidate in &candidates {
        if candidate.is_file() {
            return effigy_kernel::supervise::Executable::capture(candidate)
                .unwrap_or_else(|error| panic!("capture `{}`: {error}", candidate.display()));
        }
    }
    panic!(
        "`kernel-custody-fixture` is missing, and the release Run re-execs it for process \
         custody. `effigy test` prepares it; `effigy test:release:runtime-storage` builds it \
         and names it. Looked for: {}",
        candidates
            .iter()
            .map(|candidate| candidate.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
    );
}

// ---------------------------------------------------------------------------
// the effects a test drives
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct TestEffects {
    inner: GitEffects,
    pushes: Vec<String>,
    commit_pushes: Vec<String>,
    push_attempts: usize,
    /// Push this attempt for real, then report `Unknown`: a crash between the
    /// effect and its record.
    ambiguous_push_on: Option<usize>,
    /// Report `Unknown` on this attempt without pushing at all.
    failed_push_on: Option<usize>,
    /// Fail this task name, so a certify or verify task fails closed.
    fail_task: Option<String>,
    /// Fail to start this task name, so the release cannot learn its outcome.
    fail_task_start: Option<String>,
    /// Fail the prepare commit, so the rollback runs.
    fail_commit: bool,
    /// Fail the closeout commit push before it reaches the remote.
    fail_commit_push: bool,
    /// Print this on stdout for a `task` identity.
    task_stdout: String,
    tasks: Vec<String>,
    /// The `effigy.release.output.v1` document each task writes, by task name.
    task_outputs: BTreeMap<String, String>,
    /// Every input document a task was given, in invocation order.
    inputs: Vec<serde_json::Value>,
    /// Every output document a task wrote, in invocation order.
    outputs: Vec<String>,
    /// Scripted hosted evidence, one answer per check, oldest first.
    hosted_evidence: Vec<HostedEvidence>,
    /// The binding each hosted check was asked to honour, in check order.
    hosted_evidence_bindings: Vec<crate::hosted_evidence::HostedEvidenceBinding>,
    /// What an artifact task builds, relative to the release root.
    artifact_path: Option<String>,
    artifact_contents: Vec<u8>,
}

impl TestEffects {
    fn new(root: &Path) -> Self {
        Self {
            inner: GitEffects::with_executable(root, custody_fixture()),
            pushes: Vec::new(),
            commit_pushes: Vec::new(),
            push_attempts: 0,
            ambiguous_push_on: None,
            failed_push_on: None,
            fail_task: None,
            fail_task_start: None,
            fail_commit: false,
            fail_commit_push: false,
            task_stdout: "identity-from-task\n".to_owned(),
            tasks: Vec::new(),
            task_outputs: BTreeMap::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            hosted_evidence: Vec::new(),
            hosted_evidence_bindings: Vec::new(),
            artifact_path: None,
            artifact_contents: b"certified artifact".to_vec(),
        }
    }
}

impl ReleaseEffects for TestEffects {
    fn current_branch(&mut self, run: &mut Run<'_>) -> Result<String, ReleaseError> {
        self.inner.current_branch(run)
    }

    fn head_commit(&mut self, run: &mut Run<'_>) -> Result<String, ReleaseError> {
        self.inner.head_commit(run)
    }

    fn working_tree_clean(&mut self, run: &mut Run<'_>) -> Result<bool, ReleaseError> {
        self.inner.working_tree_clean(run)
    }

    fn local_tag_commit(
        &mut self,
        run: &mut Run<'_>,
        tag: &str,
    ) -> Result<Option<String>, ReleaseError> {
        self.inner.local_tag_commit(run, tag)
    }

    fn remote_tag_commit(
        &mut self,
        run: &mut Run<'_>,
        tag: &str,
    ) -> Result<Option<String>, ReleaseError> {
        self.inner.remote_tag_commit(run, tag)
    }

    fn create_annotated_tag(
        &mut self,
        run: &mut Run<'_>,
        tag: &str,
        commit: &str,
        message: &str,
    ) -> Result<(), ReleaseError> {
        self.inner.create_annotated_tag(run, tag, commit, message)
    }

    fn push_tag(&mut self, run: &mut Run<'_>, tag: &str) -> Result<(), ReleaseError> {
        self.push_attempts += 1;
        if self.failed_push_on == Some(self.push_attempts) {
            return Err(ReleaseError::TaskInvocation(format!(
                "simulated unknown push of `{tag}`"
            )));
        }
        self.inner.push_tag(run, tag)?;
        self.pushes.push(tag.to_owned());
        if self.ambiguous_push_on == Some(self.push_attempts) {
            return Err(ReleaseError::TaskInvocation(format!(
                "simulated interrupted push of `{tag}`"
            )));
        }
        Ok(())
    }

    fn push_commit(
        &mut self,
        run: &mut Run<'_>,
        commit: &str,
        branch: &str,
    ) -> Result<(), ReleaseError> {
        if self.fail_commit_push {
            return Err(ReleaseError::TaskInvocation(
                "simulated closeout push failure".to_owned(),
            ));
        }
        self.inner.push_commit(run, commit, branch)?;
        self.commit_pushes.push(commit.to_owned());
        Ok(())
    }

    fn commit_paths(
        &mut self,
        run: &mut Run<'_>,
        paths: &[PathBuf],
        message: &str,
    ) -> Result<String, ReleaseError> {
        if self.fail_commit {
            return Err(ReleaseError::TaskInvocation(
                "simulated prepare commit failure".to_owned(),
            ));
        }
        self.inner.commit_paths(run, paths, message)
    }

    fn discard_staged(&mut self, run: &mut Run<'_>, paths: &[PathBuf]) -> Result<(), ReleaseError> {
        self.inner.discard_staged(run, paths)
    }

    fn run_task(
        &mut self,
        _run: &mut Run<'_>,
        request: &TaskRequest,
    ) -> Result<TaskResponse, ReleaseError> {
        self.tasks.push(request.task.clone());
        let input = std::fs::read_to_string(&request.input_path).expect("release task input");
        self.inputs.push(
            serde_json::from_str(&input).unwrap_or_else(|error| panic!("input json: {error}")),
        );
        if self.fail_task_start.as_deref() == Some(request.task.as_str()) {
            return Err(ReleaseError::TaskInvocation(format!(
                "simulated start failure for `{}`",
                request.task
            )));
        }
        if self.fail_task.as_deref() == Some(request.task.as_str()) {
            return Ok(TaskResponse {
                stdout: String::new(),
                stderr: "simulated task failure".to_owned(),
                success: false,
            });
        }
        if let Some(relative) = &self.artifact_path {
            let path = request.cwd.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("artifact directory");
            }
            std::fs::write(&path, &self.artifact_contents).expect("artifact contents");
        }
        if let Some(output) = self.task_outputs.get(&request.task) {
            std::fs::write(&request.output_path, output).expect("release task output");
            self.outputs.push(output.clone());
        }
        Ok(TaskResponse {
            stdout: self.task_stdout.trim_end().to_owned(),
            stderr: String::new(),
            success: true,
        })
    }

    fn hosted_evidence(
        &mut self,
        _run: &mut Run<'_>,
        _gate: &str,
        _spec: &crate::hosted_evidence::HostedEvidenceSpec,
        commit: &str,
        binding: &crate::hosted_evidence::HostedEvidenceBinding,
    ) -> Result<HostedEvidence, ReleaseError> {
        self.hosted_evidence_bindings.push(binding.clone());
        if self.hosted_evidence.is_empty() {
            return Err(ReleaseError::TaskInvocation(
                "no scripted hosted evidence for this check".to_owned(),
            ));
        }
        let mut evidence = self.hosted_evidence.remove(0);
        if let HostedEvidence::Accepted {
            commit: accepted, ..
        } = &mut evidence
        {
            // The fake source answers for the commit it was asked about.
            *accepted = commit.to_owned();
        }
        Ok(evidence)
    }

    fn operator(&mut self, _run: &mut Run<'_>) -> Option<String> {
        Some("release tests".to_owned())
    }
}

fn request(
    fixture: &Fixture,
    action: ReleaseAction,
    to: Option<ReleaseStage>,
) -> ReleaseRunRequest {
    ReleaseRunRequest {
        repo_root: fixture.root.clone(),
        home: fixture.home.clone(),
        action,
        to,
    }
}

fn run(
    fixture: &Fixture,
    effects: &mut TestEffects,
    action: ReleaseAction,
    to: Option<ReleaseStage>,
) -> ReleaseRunOutcome {
    run_release(&request(fixture, action, to), effects).expect("release invocation")
}

fn current_record(fixture: &Fixture) -> ReleaseRecord {
    read_release_record(&fixture.home, &fixture.root)
        .expect("read release state")
        .expect("a release record")
}

fn resume(fixture: &Fixture, effects: &mut TestEffects) -> ReleaseRunOutcome {
    let id = current_record(fixture).release_id;
    run(fixture, effects, ReleaseAction::Resume { run: id }, None)
}

fn remote_tags(fixture: &Fixture) -> BTreeMap<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&fixture.remote)
        .args([
            "for-each-ref",
            "--format=%(refname:short) %(*objectname)",
            "refs/tags",
        ])
        .output()
        .expect("list remote tags");
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some((fields.next()?.to_owned(), fields.next()?.to_owned()))
        })
        .collect()
}

fn artifact_digests(outcome: &ReleaseRunOutcome) -> Vec<(String, String)> {
    let mut digests = Vec::new();
    for artifact in &outcome.record.artifacts {
        for file in &artifact.files {
            digests.push((artifact.name.clone(), file.sha256.clone()));
        }
    }
    digests
}

/// Compile one of the shipped release protocol schema documents.
fn shipped_schema(schema: &str) -> effigy_json_schema::CompiledSchema {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/knowledge/contracts/payload-schemas")
        .join(format!("{schema}.json"));
    let document: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display())),
    )
    .expect("parse shipped schema");
    effigy_json_schema::CompiledSchema::compile(&document).expect("compile shipped schema")
}

// ---------------------------------------------------------------------------
// proofs
// ---------------------------------------------------------------------------

#[test]
fn a_full_run_prepares_certifies_publishes_and_writes_a_receipt() {
    let fixture = fixture("full");
    single_identity_project(&fixture);
    let mut effects = TestEffects::new(&fixture.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());

    let outcome = run(&fixture, &mut effects, ReleaseAction::Run, None);

    assert!(fixture.root.join(".effigy/state/kernel.db").is_file());
    assert!(!fixture.home.join(".effigy/state/kernel.db").exists());
    assert!(!fixture.home.join(".effigy/state/machine.db").exists());

    assert!(
        outcome.succeeded(),
        "run failed: {:?}",
        outcome.record.errors
    );
    assert_eq!(outcome.record.status, ReleaseRunStatus::Complete);
    assert_eq!(outcome.record.stage, "closeout");
    assert_eq!(
        outcome
            .record
            .checkpoints
            .iter()
            .map(|checkpoint| checkpoint.stage.as_str())
            .collect::<Vec<_>>(),
        [
            "validate",
            "pre-gates",
            "prepare",
            "certify",
            "publish",
            "verify",
            "closeout"
        ]
    );

    // Prepare wrote the version and the changelog and committed them.
    let cargo = std::fs::read_to_string(fixture.root.join("Cargo.toml")).expect("Cargo.toml");
    assert!(cargo.contains("version = \"0.2.5\""), "{cargo}");
    let changelog =
        std::fs::read_to_string(fixture.root.join("CHANGELOG.md")).expect("CHANGELOG.md");
    assert!(changelog.contains("## [0.2.5]"), "{changelog}");
    assert!(changelog.contains("## [Unreleased]"), "{changelog}");
    let candidate = outcome
        .record
        .candidate_commit
        .clone()
        .expect("candidate commit");
    assert_eq!(git(&fixture.root, &["rev-parse", "HEAD"]), candidate);
    assert_eq!(git(&fixture.root, &["status", "--porcelain"]), "");

    // Certify ran both gate stages and the artifact task, and recorded digests.
    assert_eq!(
        outcome
            .record
            .gates
            .iter()
            .map(|gate| (gate.stage.as_str(), gate.name.as_str(), gate.passed))
            .collect::<Vec<_>>(),
        [
            ("pre-gates", "pre", true),
            ("certify", "certify-version", true)
        ]
    );
    assert_eq!(outcome.record.artifacts.len(), 1);
    assert_eq!(outcome.record.artifacts[0].files.len(), 1);
    assert!(Path::new(&outcome.record.artifacts[0].files[0].path).exists());
    assert_eq!(
        outcome.record.artifacts[0].files[0].sha256,
        sha256_bytes(b"certified artifact")
    );

    // Publish pushed the annotated tag at the candidate commit.
    let tags = remote_tags(&fixture);
    assert_eq!(tags.get("v0.2.5"), Some(&candidate));
    assert_eq!(effects.pushes, vec!["v0.2.5".to_owned()]);
    assert_eq!(
        outcome.record.target("tags").expect("tag target").state,
        RecordTargetState::Published
    );

    // Closeout pushed the release commit and wrote the receipt.
    assert_eq!(effects.commit_pushes, vec![candidate.clone()]);
    let remote_head = git(&fixture.remote, &["rev-parse", "main"]);
    assert_eq!(remote_head, candidate);
    let receipt = std::fs::read_to_string(&outcome.receipt_path).expect("receipt");
    let receipt: serde_json::Value = serde_json::from_str(&receipt).expect("receipt json");
    assert_eq!(receipt["schema"], RELEASE_RECEIPT_SCHEMA);
    assert_eq!(receipt["published"], true);
    assert_eq!(receipt["candidate"]["commit"], candidate.as_str());
    assert_eq!(
        receipt["candidate"]["artifacts"][0]["files"][0]["sha256"],
        sha256_bytes(b"certified artifact").as_str()
    );
}

/// Asserts the on-disk receipt equals what `release receipt --json` prints for
/// the same record, and returns it parsed.
fn assert_receipt_file_matches_command(
    receipt_path: &Path,
    record: &ReleaseRecord,
) -> serde_json::Value {
    let file = std::fs::read_to_string(receipt_path).expect("receipt file");
    let file: serde_json::Value = serde_json::from_str(&file).expect("receipt json");
    let command = render_release_receipt_json(&ReleaseReceipt::from_record(record));
    let command: serde_json::Value = serde_json::from_str(&command).expect("command json");
    assert_eq!(
        file, command,
        "receipt.json must match `release receipt --json`"
    );
    file
}

#[test]
fn the_receipt_file_matches_the_receipt_command_after_closeout() {
    let fixture = fixture("receipt-complete");
    single_identity_project(&fixture);
    let mut effects = TestEffects::new(&fixture.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());

    let outcome = run(&fixture, &mut effects, ReleaseAction::Run, None);
    assert!(
        outcome.succeeded(),
        "run failed: {:?}",
        outcome.record.errors
    );

    let file =
        assert_receipt_file_matches_command(&outcome.receipt_path, &current_record(&fixture));
    assert_eq!(
        file["release"]["status"],
        ReleaseRunStatus::Complete.label()
    );
    assert_eq!(file["checkpoints"].as_array().map(Vec::len), Some(7));
    assert_eq!(file["checkpoints"][6]["stage"], "closeout");
}

#[test]
fn a_paused_run_keeps_a_non_complete_receipt_until_it_completes() {
    let fixture = fixture("receipt-paused");
    single_identity_project(&fixture);
    let mut effects = TestEffects::new(&fixture.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());

    let paused = run(
        &fixture,
        &mut effects,
        ReleaseAction::Run,
        Some(ReleaseStage::PreGates),
    );
    assert!(paused.succeeded(), "run failed: {:?}", paused.record.errors);
    let file = assert_receipt_file_matches_command(&paused.receipt_path, &current_record(&fixture));
    assert_eq!(file["release"]["status"], ReleaseRunStatus::Paused.label());
    assert_eq!(file["checkpoints"].as_array().map(Vec::len), Some(2));

    let completed = resume(&fixture, &mut effects);
    assert!(
        completed.succeeded(),
        "resume failed: {:?}",
        completed.record.errors
    );
    let file =
        assert_receipt_file_matches_command(&completed.receipt_path, &current_record(&fixture));
    assert_eq!(
        file["release"]["status"],
        ReleaseRunStatus::Complete.label()
    );

    // A completed Run's re-read is idempotent: nothing rewrites the receipt.
    let before = std::fs::read(&completed.receipt_path).expect("receipt");
    let again = resume(&fixture, &mut effects);
    assert!(
        again.succeeded(),
        "re-read failed: {:?}",
        again.record.errors
    );
    assert_eq!(std::fs::read(&again.receipt_path).expect("receipt"), before);
}

#[test]
fn a_failed_closeout_never_writes_a_complete_receipt() {
    let fixture = fixture("receipt-failed-closeout");
    single_identity_project(&fixture);
    let mut effects = TestEffects::new(&fixture.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());
    effects.fail_commit_push = true;

    let outcome = run(&fixture, &mut effects, ReleaseAction::Run, None);
    assert!(!outcome.succeeded());
    assert_eq!(outcome.record.status, ReleaseRunStatus::Failed);

    let file =
        assert_receipt_file_matches_command(&outcome.receipt_path, &current_record(&fixture));
    assert_eq!(file["release"]["status"], ReleaseRunStatus::Failed.label());
    assert!(file["checkpoints"]
        .as_array()
        .expect("checkpoints")
        .iter()
        .all(|checkpoint| checkpoint["stage"] != "closeout"));
}

#[test]
fn a_missing_release_record_is_unknown_and_status_recreates_no_directory() {
    let fixture = fixture("missing-record");
    single_identity_project(&fixture);
    let mut effects = TestEffects::new(&fixture.root);
    let outcome = run(
        &fixture,
        &mut effects,
        ReleaseAction::Run,
        Some(ReleaseStage::PreGates),
    );
    let directory = outcome.receipt_path.parent().unwrap();
    assert!(directory.starts_with(
        fixture
            .root
            .canonicalize()
            .unwrap()
            .join(".effigy/state/release")
    ));
    std::fs::remove_dir_all(directory).unwrap();
    assert!(read_release_record(&fixture.home, &fixture.root)
        .unwrap_err()
        .to_string()
        .contains("record is missing"));
    assert!(!directory.exists());
    assert!(!fixture.home.join(".effigy").exists());
}

#[test]
fn every_checkpoint_resumes_with_the_same_candidate() {
    let baseline = {
        let fixture = fixture("baseline");
        single_identity_project(&fixture);
        let mut effects = TestEffects::new(&fixture.root);
        effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());
        let outcome = run(&fixture, &mut effects, ReleaseAction::Run, None);
        assert!(outcome.succeeded(), "{:?}", outcome.record.errors);
        outcome
    };

    for stop in [
        ReleaseStage::Validate,
        ReleaseStage::PreGates,
        ReleaseStage::Prepare,
        ReleaseStage::Certify,
        ReleaseStage::Publish,
        ReleaseStage::Verify,
    ] {
        let fixture = fixture(&format!("checkpoint-{}", stop.label()));
        single_identity_project(&fixture);
        let mut effects = TestEffects::new(&fixture.root);
        effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());

        let stopped = run(&fixture, &mut effects, ReleaseAction::Run, Some(stop));
        assert_eq!(
            stopped.record.status,
            ReleaseRunStatus::Paused,
            "stop at {}: {:?}",
            stop.label(),
            stopped.record.errors
        );
        assert_eq!(stopped.record.stage, stop.label());
        let prepared_at_stop = stopped.record.candidate_commit.clone();
        let versions_at_stop = stopped.record.identities.clone();
        let digests_at_stop = artifact_digests(&stopped);

        let outcome = resume(&fixture, &mut effects);
        assert!(
            outcome.succeeded(),
            "resume after {}: {:?}",
            stop.label(),
            outcome.record.errors
        );
        assert_eq!(outcome.record.status, ReleaseRunStatus::Complete);

        // The same candidate, versions and artifacts: a resume never
        // re-derives a version or rebuilds an artifact.
        if prepared_at_stop.is_some() {
            assert_eq!(
                outcome.record.candidate_commit,
                prepared_at_stop,
                "candidate changed after resuming from {}",
                stop.label()
            );
        }
        assert_eq!(
            outcome.record.identities,
            versions_at_stop,
            "versions changed after resuming from {}",
            stop.label()
        );
        if !digests_at_stop.is_empty() {
            assert_eq!(
                artifact_digests(&outcome),
                digests_at_stop,
                "artifact digests changed after resuming from {}",
                stop.label()
            );
        }
        // And the release is equal to the uninterrupted one.
        assert_eq!(
            outcome.record.identities,
            baseline.record.identities,
            "versions differ from the uninterrupted run after {}",
            stop.label()
        );
        assert_eq!(
            artifact_digests(&outcome),
            artifact_digests(&baseline),
            "artifact digests differ from the uninterrupted run after {}",
            stop.label()
        );
        let tags = remote_tags(&fixture);
        let tag = outcome.record.identities[0].tag.clone().expect("tag");
        assert_eq!(tags.get(&tag), outcome.record.candidate_commit.as_ref());
    }
}

#[test]
fn a_changed_candidate_artifact_refuses_the_publish() {
    let fixture = fixture("digest");
    single_identity_project(&fixture);
    let mut effects = TestEffects::new(&fixture.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());

    let certified = run(
        &fixture,
        &mut effects,
        ReleaseAction::Run,
        Some(ReleaseStage::Certify),
    );
    assert_eq!(certified.record.status, ReleaseRunStatus::Paused);
    let file = certified.record.artifacts[0].files[0].path.clone();
    std::fs::write(&file, b"a rebuild nobody certified").expect("rebuild the artifact");

    let outcome = resume(&fixture, &mut effects);

    assert_eq!(outcome.record.status, ReleaseRunStatus::Failed);
    assert!(
        outcome
            .record
            .errors
            .iter()
            .any(|error| error.contains("artifact digests no longer match")),
        "{:?}",
        outcome.record.errors
    );
    assert!(remote_tags(&fixture).is_empty(), "nothing may be published");
    assert_eq!(
        outcome.record.target("tags").expect("tag").state,
        RecordTargetState::Pending
    );
}

#[test]
fn an_unknown_tag_push_is_reconciled_and_never_replayed() {
    let fixture = fixture("ambiguous");
    single_identity_project(&fixture);
    let mut effects = TestEffects::new(&fixture.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());
    effects.ambiguous_push_on = Some(1);

    let first = run(&fixture, &mut effects, ReleaseAction::Run, None);
    assert_eq!(first.record.status, ReleaseRunStatus::Failed);
    assert!(
        first
            .record
            .errors
            .iter()
            .any(|error| error.contains("unknown")),
        "{:?}",
        first.record.errors
    );
    assert_eq!(
        first.record.target("tags").expect("tag").state,
        RecordTargetState::Unknown
    );
    let candidate = first.record.candidate_commit.clone().expect("candidate");
    assert_eq!(effects.pushes, vec!["v0.2.5".to_owned()]);
    assert_eq!(remote_tags(&fixture).get("v0.2.5"), Some(&candidate));

    let resumed = run(
        &fixture,
        &mut effects,
        ReleaseAction::Resume {
            run: first.record.release_id.clone(),
        },
        None,
    );

    assert!(
        resumed.succeeded(),
        "resume failed: {:?}",
        resumed.record.errors
    );
    // Reconcile looked the tag up and recorded it; it pushed nothing again.
    assert_eq!(effects.pushes, vec!["v0.2.5".to_owned()]);
    assert_eq!(effects.push_attempts, 1);
    assert_eq!(resumed.record.candidate_commit, Some(candidate.clone()));
    let destinations = &resumed.record.target("tags").expect("tag").destinations;
    assert_eq!(destinations.len(), 1);
    assert_eq!(destinations[0].destination, "v0.2.5");
    assert_eq!(destinations[0].commit, candidate);
}

#[test]
fn a_partial_tag_fan_out_resumes_with_the_remaining_identity() {
    let fixture = fixture("fanout");
    two_identity_project(&fixture);
    let mut effects = TestEffects::new(&fixture.root);
    effects.artifact_path = None;
    // The second tag push never happens: core publishes, react is unknown.
    effects.failed_push_on = Some(2);

    let first = run(&fixture, &mut effects, ReleaseAction::Run, None);
    assert_eq!(first.record.status, ReleaseRunStatus::Failed);
    let candidate = first
        .record
        .candidate_commit
        .clone()
        .unwrap_or_else(|| panic!("no candidate commit: {:?}", first.record.errors));
    assert_eq!(effects.pushes, vec!["core-v1.1.0".to_owned()]);
    let destinations = &first.record.target("tags").expect("tags").destinations;
    assert_eq!(destinations.len(), 1, "only core is recorded");

    let resumed = run(
        &fixture,
        &mut effects,
        ReleaseAction::Resume {
            run: first.record.release_id.clone(),
        },
        None,
    );

    assert!(
        resumed.succeeded(),
        "resume failed: {:?}",
        resumed.record.errors
    );
    // Core was not pushed again; only react was.
    assert_eq!(
        effects.pushes,
        vec!["core-v1.1.0".to_owned(), "react-v1.1.0".to_owned()]
    );
    let tags = remote_tags(&fixture);
    assert_eq!(tags.get("core-v1.1.0"), Some(&candidate));
    assert_eq!(tags.get("react-v1.1.0"), Some(&candidate));
    let destinations = &resumed.record.target("tags").expect("tags").destinations;
    assert_eq!(destinations.len(), 2);
    assert_eq!(
        resumed.record.status,
        ReleaseRunStatus::Complete,
        "the release commit is pushed at closeout"
    );
    assert_eq!(effects.commit_pushes.len(), 1);
}

#[test]
fn a_failed_prepare_rolls_the_tree_back() {
    let fixture = fixture("rollback");
    single_identity_project(&fixture);
    // A read-only changelog fails the write after the version file was already
    // written, so the rollback has something to restore.
    let changelog = fixture.root.join("CHANGELOG.md");
    let mut permissions = std::fs::metadata(&changelog)
        .expect("metadata")
        .permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o444);
    }
    std::fs::set_permissions(&changelog, permissions).expect("make read-only");
    // A privileged test account can write a read-only file; there is then no
    // failing write to prove, so the proof is skipped rather than faked.
    let enforced = std::fs::write(&changelog, CHANGELOG).is_err();

    let mut effects = TestEffects::new(&fixture.root);
    let outcome = run(&fixture, &mut effects, ReleaseAction::Run, None);

    let mut permissions = std::fs::metadata(&changelog)
        .expect("metadata")
        .permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o644);
    }
    std::fs::set_permissions(&changelog, permissions).expect("restore permissions");

    if enforced {
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
        let cargo = std::fs::read_to_string(fixture.root.join("Cargo.toml")).expect("Cargo.toml");
        assert!(
            cargo.contains("version = \"0.2.4\""),
            "the version file must be back at its pre-prepare version: {cargo}"
        );
        assert_eq!(
            git(&fixture.root, &["status", "--porcelain"]),
            "",
            "the working tree must be clean after a rolled-back prepare"
        );
        assert!(remote_tags(&fixture).is_empty());
    }
}

#[test]
fn a_failed_prepare_commit_rolls_the_tree_back() {
    let fixture = fixture("rollback-commit");
    single_identity_project(&fixture);
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
    let cargo = std::fs::read_to_string(fixture.root.join("Cargo.toml")).expect("Cargo.toml");
    assert!(cargo.contains("version = \"0.2.4\""), "{cargo}");
    assert_eq!(git(&fixture.root, &["status", "--porcelain"]), "");
    assert!(remote_tags(&fixture).is_empty());
}

#[test]
fn validate_refuses_a_dirty_tree() {
    let dirty = fixture("dirty");
    single_identity_project(&dirty);
    write(&dirty.root, "untracked.txt", "pending work\n");
    let mut effects = TestEffects::new(&dirty.root);
    let outcome = run(&dirty, &mut effects, ReleaseAction::Run, None);
    assert_eq!(outcome.record.status, ReleaseRunStatus::Failed);
    assert!(
        outcome.record.errors.iter().any(
            |error| error.contains("clean working tree") || error.contains("unexpected change")
        ),
        "{:?}",
        outcome.record.errors
    );
}

#[test]
fn an_approval_checkpoint_stops_the_run_until_it_is_approved() {
    let fixture = fixture("approval");
    two_identity_project(&fixture);
    // An explicit list, not the §10 ruling 3 default (tag-only releases are
    // approved by default not at all), so the checkpoint is where this test
    // says it is.
    let manifest = std::fs::read_to_string(fixture.root.join("effigy.toml")).expect("manifest");
    std::fs::write(
        fixture.root.join("effigy.toml"),
        manifest.replace(
            "changelog = \"CHANGELOG.md\"",
            "changelog = \"CHANGELOG.md\"\napprove = [\"publish\"]",
        ),
    )
    .expect("write manifest");
    commit_and_push(&fixture);

    let mut effects = TestEffects::new(&fixture.root);
    let paused = run(&fixture, &mut effects, ReleaseAction::Run, None);
    assert_eq!(paused.record.status, ReleaseRunStatus::AwaitingApproval);
    assert_eq!(paused.record.waiting_on.as_deref(), Some("publish"));
    assert!(effects.pushes.is_empty());

    let mut not_approved = TestEffects::new(&fixture.root);
    not_approved.fail_task = None;
    let refused = run(
        &fixture,
        &mut not_approved,
        ReleaseAction::Resume {
            run: paused.record.release_id.clone(),
        },
        None,
    );
    assert_eq!(refused.record.status, ReleaseRunStatus::AwaitingApproval);

    let approved = run(
        &fixture,
        &mut effects,
        ReleaseAction::Approve {
            run: paused.record.release_id.clone(),
        },
        None,
    );
    assert!(
        approved.succeeded(),
        "approve failed: {:?}",
        approved.record.errors
    );
    assert_eq!(approved.record.approvals.len(), 1);
    assert_eq!(approved.record.approvals[0].stage, "publish");
    assert_eq!(approved.record.approvals[0].approved_by, "release tests");
    assert_eq!(effects.pushes.len(), 2);
}

#[test]
fn a_certify_gate_failure_fails_closed_before_publish() {
    let fixture = fixture("gate");
    single_identity_project(&fixture);
    write(&fixture.root, "gates/certify.sh", "#!/bin/sh\nexit 3\n");
    commit_and_push(&fixture);
    let mut effects = TestEffects::new(&fixture.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());

    let outcome = run(&fixture, &mut effects, ReleaseAction::Run, None);
    assert_eq!(outcome.record.status, ReleaseRunStatus::Failed);
    assert!(
        outcome
            .record
            .errors
            .iter()
            .any(|error| error.contains("certify gates failed")),
        "{:?}",
        outcome.record.errors
    );
    assert!(remote_tags(&fixture).is_empty());
}

#[test]
fn a_failed_verify_leaves_the_release_published_but_unverified() {
    let fixture = fixture("verify");
    single_identity_project(&fixture);
    let manifest = std::fs::read_to_string(fixture.root.join("effigy.toml")).expect("manifest");
    std::fs::write(
        fixture.root.join("effigy.toml"),
        manifest.replace(
            "[release.targets.tags]\nkind = \"tag\"",
            "[release.targets.tags]\nkind = \"tag\"\nverify = \"release:health\"",
        ),
    )
    .expect("manifest");
    commit_and_push(&fixture);

    let mut effects = TestEffects::new(&fixture.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());
    effects.fail_task = Some("release:health".to_owned());

    let outcome = run(&fixture, &mut effects, ReleaseAction::Run, None);

    assert_eq!(outcome.record.status, ReleaseRunStatus::Complete);
    assert_eq!(outcome.record.verifications.len(), 1);
    assert!(!outcome.record.verifications[0].passed);
    let receipt = outcome.receipt();
    assert!(receipt.published);
    assert!(receipt.unverified);
    assert_eq!(receipt.errors.len(), 1);
}

#[test]
fn a_consumer_shorthand_table_prepares_unchanged() {
    // Contract 054 §4.1: the eleven projects using `[release]` keep working
    // through the single-identity shorthand. Their own gates and artifacts are
    // project work, so this proof drives each real table to `prepare`.
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/release_platform");
    for consumer in ["longhorn", "poodle", "underlay"] {
        let fixture = fixture(&format!("consumer-{consumer}"));
        copy_tree(&fixtures.join(consumer), &fixture.root);
        // The consumer fixtures trim their workspaces to what resolution
        // reads. A prepare syncs the lockfile, so give the copied workspace the
        // member crate the real project has and keep its `[release]` table
        // untouched.
        if consumer != "poodle" {
            std::fs::write(
                fixture.root.join("Cargo.toml"),
                std::fs::read_to_string(fixture.root.join("Cargo.toml"))
                    .expect("Cargo.toml")
                    .replace("members = []", "members = [\"crates/member\"]"),
            )
            .expect("Cargo.toml members");
            write(
                &fixture.root,
                "crates/member/Cargo.toml",
                "[package]\nname = \"member\"\nversion.workspace = true\nedition.workspace = true\n",
            );
            write(&fixture.root, "crates/member/src/lib.rs", "");
            // The real lockfile lists the workspace members; the trimmed
            // fixture kept only its header.
            let version = if consumer == "underlay" {
                "0.6.1"
            } else {
                "0.3.2"
            };
            std::fs::write(
                fixture.root.join("Cargo.lock"),
                format!(
                    "# This file is automatically @generated by Cargo.\n# It is not intended for manual editing.\nversion = 4\n\n[[package]]\nname = \"member\"\nversion = \"{version}\"\n"
                ),
            )
            .expect("Cargo.lock");
        }
        commit_and_push(&fixture);
        let mut effects = TestEffects::new(&fixture.root);
        let outcome = run(
            &fixture,
            &mut effects,
            ReleaseAction::Run,
            Some(ReleaseStage::Prepare),
        );
        assert!(
            outcome.succeeded(),
            "{consumer} prepare failed: {:?}",
            outcome.record.errors
        );
        assert_eq!(outcome.record.stage, "prepare", "{consumer}");
        assert_eq!(outcome.record.identities.len(), 1, "{consumer}");
        assert_eq!(outcome.record.identities[0].kind, "semver", "{consumer}");
        assert!(outcome.record.candidate_commit.is_some(), "{consumer}");
        assert_eq!(
            git(&fixture.root, &["status", "--porcelain"]),
            "",
            "{consumer}"
        );
    }
}

fn copy_tree(source: &Path, target: &Path) {
    for entry in std::fs::read_dir(source).expect("read fixture") {
        let entry = entry.expect("fixture entry");
        let path = entry.path();
        let destination = target.join(entry.file_name());
        if path.is_dir() {
            std::fs::create_dir_all(&destination).expect("fixture directory");
            copy_tree(&path, &destination);
        } else {
            std::fs::create_dir_all(destination.parent().expect("parent")).expect("parent");
            std::fs::copy(&path, &destination).expect("copy fixture file");
        }
    }
}

#[test]
fn checkout_release_resumes_while_another_checkout_writer_is_stalled() {
    use effigy_kernel::ctx::*;
    use effigy_kernel::id::{ResourceId, Scope};
    use effigy_kernel::location::CheckoutLocation;
    use effigy_kernel::{root::StateRoot, store::Store};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    };
    use std::time::{Duration, Instant};
    let first = fixture("writer-stall");
    let mut second = fixture("independent-resume");
    second.home = first.home.clone();
    single_identity_project(&first);
    single_identity_project(&second);
    let mut first_effects = TestEffects::new(&first.root);
    let mut second_effects = TestEffects::new(&second.root);
    run(
        &first,
        &mut first_effects,
        ReleaseAction::Run,
        Some(ReleaseStage::Validate),
    );
    let paused = run(
        &second,
        &mut second_effects,
        ReleaseAction::Run,
        Some(ReleaseStage::Validate),
    );
    let op = OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(60)),
        CancelToken::default(),
        Limits {
            capture_tail: 4096,
            log_rotate: 4096,
            log_keep: 1,
            line_max: 4096,
            queue_bytes: 4096,
            probe_concurrency: 1,
        },
        EffectClass::Mutate,
        Placement::Host,
    );
    let cx = op.effects().unwrap();
    let location = CheckoutLocation {
        root: first.root.clone(),
    };
    let root = StateRoot::checkout_effects(&cx, &first.home, &location).unwrap();
    let owner = ReleaseSession::open(&first.home, &first.root)
        .unwrap()
        .owner;
    let Scope::Checkout(checkout) = owner.scope() else {
        panic!("local release owner");
    };
    let stalled = ResourceId::store(Scope::Checkout(checkout), "stalled-writer-proof").unwrap();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let active = Arc::new(AtomicBool::new(false));
    let writer_active = active.clone();
    let writer = std::thread::spawn(move || {
        let mut store = Store::open(&cx, &root).unwrap();
        store
            .txn(&cx, |txn| {
                txn.resource(&stalled, "private fixture")?;
                txn.next_generation(&stalled)?;
                writer_active.store(true, Ordering::SeqCst);
                ready_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(60)).unwrap();
                writer_active.store(false, Ordering::SeqCst);
                Ok::<_, effigy_kernel::store::StoreError>(())
            })
            .unwrap();
    });
    struct ReleaseWriter(
        Option<mpsc::Sender<()>>,
        Option<std::thread::JoinHandle<()>>,
    );
    impl Drop for ReleaseWriter {
        fn drop(&mut self) {
            if let Some(sender) = self.0.take() {
                let _ = sender.send(());
            }
            if let Some(writer) = self.1.take() {
                writer.join().unwrap();
            }
        }
    }
    let _release = ReleaseWriter(Some(release_tx), Some(writer));
    ready_rx.recv_timeout(Duration::from_secs(30)).unwrap();
    let resumed = run(
        &second,
        &mut second_effects,
        ReleaseAction::Resume {
            run: paused.record.release_id.clone(),
        },
        Some(ReleaseStage::PreGates),
    );
    assert_eq!(resumed.record.release_id, paused.record.release_id);
    assert_eq!(resumed.record.stage, "pre-gates");
    assert!(
        active.load(Ordering::SeqCst),
        "the other checkout's write transaction remains held"
    );
    assert!(
        !first.home.join(".effigy").exists(),
        "neither release initializes machine runtime state"
    );
}

// ---------------------------------------------------------------------------
// task targets, check/verify, decisions and hosted waits (contract 054 §3, §6, §7)
// ---------------------------------------------------------------------------

/// Append `[release]` configuration to the fixture's manifest.
fn append_manifest(fixture: &Fixture, table: &str) {
    let manifest = std::fs::read_to_string(fixture.root.join("effigy.toml")).expect("manifest");
    std::fs::write(
        fixture.root.join("effigy.toml"),
        format!("{}\n{table}\n", manifest.trim_end()),
    )
    .expect("write manifest");
}

const NPM_TARGET: &str = r#"[release.targets.npm]
kind = "task"
task = "release:publish:npm"
verify = "release:verify:npm"
"#;

const NPM_PUBLISH_ONLY: &str = r#"[release.targets.npm]
kind = "task"
task = "release:publish:npm"
"#;

const NPM_TARGET_WITH_CHECK: &str = r#"[release.targets.npm]
kind = "task"
task = "release:publish:npm"
check = "release:check:npm"
verify = "release:verify:npm"
"#;

fn npm_output(committed: &str, destinations: &[&str]) -> String {
    serde_json::json!({
        "schema": RELEASE_TASK_OUTPUT_SCHEMA,
        "committed": match committed {
            "true" => serde_json::json!(true),
            "false" => serde_json::json!(false),
            _ => serde_json::json!("unknown"),
        },
        "destinations": destinations,
        "note": "fake npm registry",
    })
    .to_string()
}

/// Drive a Run past the §10 ruling 3 approval checkpoint that a non-`tag`
/// target turns on.
fn approve_publish(
    fixture: &Fixture,
    effects: &mut TestEffects,
    paused: &ReleaseRunOutcome,
) -> ReleaseRunOutcome {
    assert_eq!(
        paused.record.status,
        ReleaseRunStatus::AwaitingApproval,
        "a non-`tag` target defaults the publish approval on: {:?}",
        paused.record.errors
    );
    assert_eq!(paused.record.waiting_on.as_deref(), Some("publish"));
    run(
        fixture,
        effects,
        ReleaseAction::Approve {
            run: paused.record.release_id.clone(),
        },
        None,
    )
}

#[test]
fn a_task_target_publishes_the_certified_candidate() {
    let fixture = fixture("task-target");
    single_identity_project(&fixture);
    append_manifest(&fixture, NPM_TARGET);
    commit_and_push(&fixture);

    let mut effects = TestEffects::new(&fixture.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());
    effects.task_outputs.insert(
        "release:publish:npm".to_owned(),
        npm_output("true", &["fixture@0.2.5"]),
    );

    let paused = run(&fixture, &mut effects, ReleaseAction::Run, None);
    assert!(
        !effects
            .tasks
            .iter()
            .any(|task| task == "release:publish:npm"),
        "nothing publishes before the approval: {:?}",
        effects.tasks
    );
    assert!(remote_tags(&fixture).is_empty());

    let approved = approve_publish(&fixture, &mut effects, &paused);
    assert!(
        approved.succeeded(),
        "approve failed: {:?}",
        approved.record.errors
    );
    assert_eq!(approved.record.status, ReleaseRunStatus::Complete);
    let candidate = approved
        .record
        .candidate_commit
        .clone()
        .expect("candidate commit");

    // The task received the certified candidate and the target name (054 §6).
    let publish_input = effects
        .inputs
        .iter()
        .find(|input| input["target"] == "npm" && input["stage"] == "publish")
        .unwrap_or_else(|| panic!("no npm publish input: {:?}", effects.inputs));
    assert_eq!(publish_input["schema"], RELEASE_TASK_INPUT_SCHEMA);
    assert_eq!(publish_input["candidate_commit"], candidate.as_str());
    assert_eq!(publish_input["subject"]["kind"], "target");
    assert_eq!(publish_input["subject"]["name"], "npm");
    let artifact = &approved.record.artifacts[0];
    assert_eq!(
        publish_input["artifacts"][0]["files"][0]["sha256"],
        artifact.files[0].sha256.as_str()
    );
    assert_eq!(
        publish_input["identities"][0]["identity"],
        approved.record.identities[0]
            .identity
            .as_deref()
            .expect("identity")
    );

    // The declared result is recorded on the target and in the receipt.
    let npm = approved.record.target("npm").expect("npm target");
    assert_eq!(npm.state, RecordTargetState::Published);
    assert_eq!(npm.destinations.len(), 1);
    assert_eq!(npm.destinations[0].destination, "fixture@0.2.5");
    assert_eq!(npm.destinations[0].commit, candidate);
    assert_eq!(npm.destinations[0].identity, None);
    assert_eq!(npm.note.as_deref(), Some("fake npm registry"));

    let receipt = approved.receipt();
    assert!(receipt.published);
    assert!(!receipt.unverified);
    assert_eq!(receipt.verification.len(), 1);
    assert_eq!(receipt.verification[0].target, "npm");
    assert!(receipt.verification[0].passed);
    assert_eq!(
        effects
            .inputs
            .iter()
            .filter(|input| input["target"] == "npm" && input["stage"] == "verify")
            .count(),
        1,
        "the target's verify task ran against the published candidate"
    );
    // The tag target published first, so later targets can rely on the tag
    // existing (§10 ruling 1).
    assert_eq!(remote_tags(&fixture).get("v0.2.5"), Some(&candidate));

    // Every document the task protocol exchanged satisfies the shipped
    // `effigy.release.input.v1` and `effigy.release.output.v1` schemas.
    let input_schema = shipped_schema(RELEASE_TASK_INPUT_SCHEMA);
    for input in &effects.inputs {
        let errors = input_schema.validate(input);
        assert!(errors.is_empty(), "{errors:?} in {input}");
    }
    let output_schema = shipped_schema(RELEASE_TASK_OUTPUT_SCHEMA);
    for output in &effects.outputs {
        let document: serde_json::Value = serde_json::from_str(output).expect("output document");
        let errors = output_schema.validate(&document);
        assert!(errors.is_empty(), "{errors:?} in {document}");
    }
    assert!(!effects.outputs.is_empty());
}

#[test]
fn a_task_target_that_reports_it_did_not_publish_fails_closed() {
    let fixture = fixture("task-not-published");
    single_identity_project(&fixture);
    append_manifest(&fixture, NPM_TARGET);
    commit_and_push(&fixture);

    let mut effects = TestEffects::new(&fixture.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());
    effects
        .task_outputs
        .insert("release:publish:npm".to_owned(), npm_output("false", &[]));

    let paused = run(&fixture, &mut effects, ReleaseAction::Run, None);
    let outcome = approve_publish(&fixture, &mut effects, &paused);

    assert_eq!(outcome.record.status, ReleaseRunStatus::Failed);
    assert!(
        outcome
            .record
            .errors
            .iter()
            .any(|error| error.contains("did not publish")),
        "{:?}",
        outcome.record.errors
    );
    assert_eq!(
        outcome.record.target("npm").expect("npm").state,
        RecordTargetState::Failed
    );
    assert!(
        outcome
            .record
            .target("npm")
            .expect("npm")
            .destinations
            .is_empty(),
        "a refused publish records no destination"
    );
    // The release stops after the failure: no verify and no closeout.
    assert!(outcome.record.verifications.is_empty());
    assert_eq!(outcome.record.status, ReleaseRunStatus::Failed);
    assert!(!outcome.receipt().published);
}

#[test]
fn an_unknown_task_publish_without_a_check_stops_for_an_operator_decision() {
    let fixture = fixture("task-decision");
    single_identity_project(&fixture);
    append_manifest(&fixture, NPM_TARGET);
    commit_and_push(&fixture);

    let mut effects = TestEffects::new(&fixture.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());
    effects
        .task_outputs
        .insert("release:publish:npm".to_owned(), npm_output("unknown", &[]));

    let paused = run(&fixture, &mut effects, ReleaseAction::Run, None);
    let waiting = approve_publish(&fixture, &mut effects, &paused);
    // No `check` task to reconcile it, so the Run stops for the operator in
    // this same invocation instead of failing first (054 §7).
    assert_eq!(waiting.record.status, ReleaseRunStatus::AwaitingDecision);
    assert_eq!(waiting.record.waiting_on.as_deref(), Some("npm"));
    assert!(waiting.succeeded(), "a waiting Run is not a failing one");
    assert_eq!(
        waiting.record.target("npm").expect("npm").state,
        RecordTargetState::Unknown
    );
    assert_eq!(
        effects
            .tasks
            .iter()
            .filter(|task| *task == "release:publish:npm")
            .count(),
        1
    );

    // A resume re-enters publish, finds the unknown outcome and waits for the
    // operator again instead of retrying blind (§7).
    let resumed = resume(&fixture, &mut effects);
    assert_eq!(resumed.record.status, ReleaseRunStatus::AwaitingDecision);
    assert_eq!(
        effects
            .tasks
            .iter()
            .filter(|task| *task == "release:publish:npm")
            .count(),
        1,
        "the unknown publish was never retried"
    );

    // The operator records "published": the Run continues without publishing
    // again and the receipt says who decided and when.
    let decided = run(
        &fixture,
        &mut effects,
        ReleaseAction::Decide {
            run: resumed.record.release_id.clone(),
            decision: TargetDecision::Published,
        },
        None,
    );
    assert!(
        decided.succeeded(),
        "decide failed: {:?}",
        decided.record.errors
    );
    assert_eq!(decided.record.status, ReleaseRunStatus::Complete);
    assert_eq!(
        effects
            .tasks
            .iter()
            .filter(|task| *task == "release:publish:npm")
            .count(),
        1
    );
    assert_eq!(
        decided.record.target("npm").expect("npm").state,
        RecordTargetState::Published
    );
    assert_eq!(decided.record.decisions.len(), 1);
    let decision = &decided.record.decisions[0];
    assert_eq!(decision.target, "npm");
    assert_eq!(decision.decision, "published");
    assert_eq!(decision.decided_by, "release tests");
    assert!(!decision.decided_at.is_empty());
    let receipt = decided.receipt();
    assert!(receipt.published);
    assert_eq!(receipt.decisions.len(), 1);
    assert!(render_release_receipt_text(&receipt).contains("decisions:"));
}

#[test]
fn an_unknown_task_publish_with_a_check_reconciles_before_any_retry() {
    // The check reports the candidate is already there: publish again would be
    // a blind retry, so it publishes nothing.
    let already = fixture("task-check-published");
    single_identity_project(&already);
    append_manifest(&already, NPM_TARGET_WITH_CHECK);
    commit_and_push(&already);

    let mut effects = TestEffects::new(&already.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());
    effects
        .task_outputs
        .insert("release:publish:npm".to_owned(), npm_output("unknown", &[]));
    effects.task_outputs.insert(
        "release:check:npm".to_owned(),
        npm_output("true", &["fixture@0.2.5"]),
    );

    let paused = run(&already, &mut effects, ReleaseAction::Run, None);
    let unknown = approve_publish(&already, &mut effects, &paused);
    assert_eq!(
        unknown.record.target("npm").expect("npm").state,
        RecordTargetState::Unknown
    );

    let reconciled = resume(&already, &mut effects);
    assert!(
        reconciled.succeeded(),
        "resume failed: {:?}",
        reconciled.record.errors
    );
    assert_eq!(reconciled.record.status, ReleaseRunStatus::Complete);
    assert_eq!(
        reconciled.record.target("npm").expect("npm").state,
        RecordTargetState::Published
    );
    assert_eq!(
        reconciled.record.target("npm").expect("npm").destinations[0].destination,
        "fixture@0.2.5"
    );
    assert_eq!(
        effects
            .tasks
            .iter()
            .filter(|task| *task == "release:publish:npm")
            .count(),
        1,
        "the check reconciled the outcome; nothing was published twice"
    );
    assert_eq!(
        effects
            .tasks
            .iter()
            .filter(|task| *task == "release:check:npm")
            .count(),
        1
    );

    // The check reports the candidate is not there: publishing now is not
    // blind, and the retry is recorded.
    let missing = fixture("task-check-missing");
    single_identity_project(&missing);
    append_manifest(&missing, NPM_TARGET_WITH_CHECK);
    commit_and_push(&missing);

    let mut effects = TestEffects::new(&missing.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());
    effects
        .task_outputs
        .insert("release:publish:npm".to_owned(), npm_output("unknown", &[]));
    effects
        .task_outputs
        .insert("release:check:npm".to_owned(), npm_output("false", &[]));

    let paused = run(&missing, &mut effects, ReleaseAction::Run, None);
    let unknown = approve_publish(&missing, &mut effects, &paused);
    assert_eq!(unknown.record.status, ReleaseRunStatus::Failed);

    // A second publish declares success this time.
    effects.task_outputs.insert(
        "release:publish:npm".to_owned(),
        npm_output("true", &["fixture@0.2.5"]),
    );
    let retried = resume(&missing, &mut effects);
    assert!(
        retried.succeeded(),
        "resume failed: {:?}",
        retried.record.errors
    );
    assert_eq!(retried.record.status, ReleaseRunStatus::Complete);
    assert_eq!(
        effects
            .tasks
            .iter()
            .filter(|task| *task == "release:check:npm")
            .count(),
        1
    );
    assert_eq!(
        effects
            .tasks
            .iter()
            .filter(|task| *task == "release:publish:npm")
            .count(),
        2,
        "the check said it was not published, so the retry published it"
    );
    assert_eq!(
        retried.record.target("npm").expect("npm").state,
        RecordTargetState::Published
    );
}

#[test]
fn a_task_publish_whose_outcome_is_lost_leaves_unknown_and_resumes_through_check() {
    let fixture = fixture("task-crash");
    single_identity_project(&fixture);
    append_manifest(&fixture, NPM_TARGET_WITH_CHECK);
    commit_and_push(&fixture);

    let mut effects = TestEffects::new(&fixture.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());
    // The publish is journalled, then the invocation loses the task: no result
    // is ever recorded, as a crash mid-publish would leave it.
    effects.fail_task_start = Some("release:publish:npm".to_owned());
    effects.task_outputs.insert(
        "release:check:npm".to_owned(),
        npm_output("true", &["fixture@0.2.5"]),
    );

    let paused = run(&fixture, &mut effects, ReleaseAction::Run, None);
    let crashed = approve_publish(&fixture, &mut effects, &paused);
    assert_eq!(crashed.record.status, ReleaseRunStatus::Failed);
    // The target is written `Unknown` before the task starts, so a crash never
    // leaves a target that a resume would publish again blind.
    assert_eq!(
        crashed.record.target("npm").expect("npm").state,
        RecordTargetState::Unknown
    );
    assert!(crashed
        .record
        .target("npm")
        .expect("npm")
        .destinations
        .is_empty());

    // The resume reconciles through the check task, which reports the
    // candidate is already there, so the publish never runs a second time.
    effects.fail_task_start = None;
    let resumed = resume(&fixture, &mut effects);
    assert!(
        resumed.succeeded(),
        "resume failed: {:?}",
        resumed.record.errors
    );
    assert_eq!(
        resumed.record.target("npm").expect("npm").state,
        RecordTargetState::Published
    );
    assert_eq!(
        effects
            .tasks
            .iter()
            .filter(|task| *task == "release:publish:npm")
            .count(),
        1,
        "the lost publish was reconciled, not retried blind"
    );
    assert_eq!(
        effects
            .tasks
            .iter()
            .filter(|task| *task == "release:check:npm")
            .count(),
        1
    );
    assert_eq!(
        resumed.record.target("npm").expect("npm").destinations[0].destination,
        "fixture@0.2.5"
    );
}

#[test]
fn an_unknown_target_stops_the_publish_fan_out_before_later_targets() {
    let fixture = fixture("task-fanout-stop");
    single_identity_project(&fixture);
    // Alphabetical target order, so `npm` publishes before `zeta`.
    let mut manifest = String::from(NPM_PUBLISH_ONLY);
    manifest
        .push_str("\n[release.targets.zeta]\nkind = \"task\"\ntask = \"release:publish:zeta\"\n");
    append_manifest(&fixture, &manifest);
    commit_and_push(&fixture);

    let mut effects = TestEffects::new(&fixture.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());
    effects
        .task_outputs
        .insert("release:publish:npm".to_owned(), npm_output("unknown", &[]));
    effects.task_outputs.insert(
        "release:publish:zeta".to_owned(),
        npm_output("true", &["zeta@0.2.5"]),
    );

    let paused = run(&fixture, &mut effects, ReleaseAction::Run, None);
    let waiting = approve_publish(&fixture, &mut effects, &paused);
    assert_eq!(waiting.record.status, ReleaseRunStatus::AwaitingDecision);
    assert_eq!(waiting.record.waiting_on.as_deref(), Some("npm"));
    assert!(
        !effects
            .tasks
            .iter()
            .any(|task| task == "release:publish:zeta"),
        "a later target must not publish while the outcome is unresolved: {:?}",
        effects.tasks
    );
    assert_eq!(
        waiting.record.target("zeta").expect("zeta").state,
        RecordTargetState::Pending
    );

    // Once the operator records the outcome, the fan-out continues with the
    // target it had not reached.
    let decided = run(
        &fixture,
        &mut effects,
        ReleaseAction::Decide {
            run: waiting.record.release_id.clone(),
            decision: TargetDecision::Published,
        },
        None,
    );
    assert!(
        decided.succeeded(),
        "decide failed: {:?}",
        decided.record.errors
    );
    assert_eq!(decided.record.status, ReleaseRunStatus::Complete);
    assert_eq!(
        decided.record.target("npm").expect("npm").state,
        RecordTargetState::Published
    );
    assert_eq!(
        decided.record.target("zeta").expect("zeta").destinations[0].destination,
        "zeta@0.2.5"
    );
    assert_eq!(
        effects
            .tasks
            .iter()
            .filter(|task| *task == "release:publish:npm")
            .count(),
        1
    );
    assert_eq!(
        effects
            .tasks
            .iter()
            .filter(|task| *task == "release:publish:zeta")
            .count(),
        1
    );
}

#[test]
fn an_unstartable_task_publish_without_a_check_waits_for_the_operator() {
    let fixture = fixture("task-start-failure");
    single_identity_project(&fixture);
    append_manifest(&fixture, NPM_PUBLISH_ONLY);
    commit_and_push(&fixture);

    let mut effects = TestEffects::new(&fixture.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());
    // The publish cannot be started at all: nothing can be said about the
    // destination, which is an unknown outcome like any other.
    effects.fail_task_start = Some("release:publish:npm".to_owned());

    let paused = run(&fixture, &mut effects, ReleaseAction::Run, None);
    let waiting = approve_publish(&fixture, &mut effects, &paused);
    assert_eq!(waiting.record.status, ReleaseRunStatus::AwaitingDecision);
    assert_eq!(waiting.record.waiting_on.as_deref(), Some("npm"));
    assert_eq!(
        waiting.record.target("npm").expect("npm").state,
        RecordTargetState::Unknown
    );

    // The operator can decide right away; "not published" publishes it.
    effects.fail_task_start = None;
    effects.task_outputs.insert(
        "release:publish:npm".to_owned(),
        npm_output("true", &["fixture@0.2.5"]),
    );
    let decided = run(
        &fixture,
        &mut effects,
        ReleaseAction::Decide {
            run: waiting.record.release_id.clone(),
            decision: TargetDecision::NotPublished,
        },
        None,
    );
    assert!(
        decided.succeeded(),
        "decide failed: {:?}",
        decided.record.errors
    );
    assert_eq!(decided.record.status, ReleaseRunStatus::Complete);
    assert_eq!(
        decided.record.target("npm").expect("npm").state,
        RecordTargetState::Published
    );
    assert_eq!(decided.record.decisions[0].decision, "not-published");
    assert_eq!(
        effects
            .tasks
            .iter()
            .filter(|task| *task == "release:publish:npm")
            .count(),
        2,
        "the unstartable attempt and the publish the operator asked for"
    );
}

#[test]
fn a_partial_task_target_fan_out_resumes_with_the_remaining_target() {
    let fixture = fixture("task-fanout");
    single_identity_project(&fixture);
    append_manifest(
        &fixture,
        &format!(
            "{NPM_TARGET}\n[release.targets.gh]\nkind = \"task\"\ntask = \"release:publish:gh\"\ncheck = \"release:check:gh\"\n"
        ),
    );
    commit_and_push(&fixture);

    let mut effects = TestEffects::new(&fixture.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());
    effects.task_outputs.insert(
        "release:publish:npm".to_owned(),
        npm_output("true", &["fixture@0.2.5"]),
    );
    // The second target's publish outcome is unknown: npm stays recorded.
    effects
        .task_outputs
        .insert("release:publish:gh".to_owned(), npm_output("unknown", &[]));
    effects.task_outputs.insert(
        "release:check:gh".to_owned(),
        npm_output("true", &["https://example.test/releases/v0.2.5"]),
    );

    let paused = run(&fixture, &mut effects, ReleaseAction::Run, None);
    let partial = approve_publish(&fixture, &mut effects, &paused);
    assert_eq!(partial.record.status, ReleaseRunStatus::Failed);
    assert_eq!(
        partial.record.target("npm").expect("npm").state,
        RecordTargetState::Published
    );
    assert_eq!(
        partial.record.target("gh").expect("gh").state,
        RecordTargetState::Unknown
    );

    let resumed = resume(&fixture, &mut effects);
    assert!(
        resumed.succeeded(),
        "resume failed: {:?}",
        resumed.record.errors
    );
    assert_eq!(
        resumed.record.target("npm").expect("npm").state,
        RecordTargetState::Published
    );
    assert_eq!(
        resumed.record.target("gh").expect("gh").state,
        RecordTargetState::Published
    );
    assert_eq!(
        effects
            .tasks
            .iter()
            .filter(|task| *task == "release:publish:npm")
            .count(),
        1,
        "a target that succeeded is not published again"
    );
    assert_eq!(
        resumed.record.target("gh").expect("gh").destinations[0].destination,
        "https://example.test/releases/v0.2.5"
    );
    assert!(resumed.receipt().published);
}

#[test]
fn a_hosted_wait_stops_the_run_without_holding_a_process_open() {
    let waited = fixture("hosted-wait");
    single_identity_project(&waited);
    append_manifest(
        &waited,
        "[release.gates.hosted]\ncommand = \"exit 7\"\nreuse-hosted-evidence = true\nstage = \"certify\"\n\n[release.hosted-evidence]\nworkflow = \"ci.yml\"\n",
    );
    commit_and_push(&waited);

    let mut effects = TestEffects::new(&waited.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());
    effects.hosted_evidence = vec![HostedEvidence::Waiting {
        detail: "hosted evidence for gate `hosted` is pending: run … status is `in_progress`."
            .to_owned(),
        run: None,
    }];

    let waiting = run(&waited, &mut effects, ReleaseAction::Run, None);
    assert!(
        waiting.succeeded(),
        "a hosted wait is not a failure: {:?}",
        waiting.record.errors
    );
    assert_eq!(
        waiting.record.status,
        ReleaseRunStatus::AwaitingHostedEvidence
    );
    assert_eq!(waiting.record.waiting_on.as_deref(), Some("hosted"));
    // The gate stage did not complete: the last checkpoint is still prepare
    // and nothing downstream of the gate stage ran.
    assert_eq!(waiting.record.stage, "prepare");
    assert!(waiting
        .record
        .gates
        .iter()
        .all(|gate| gate.stage != "certify" || gate.passed));
    assert!(
        waiting.record.artifacts.is_empty(),
        "a hosted wait captures no artifacts"
    );
    assert!(
        !effects.tasks.iter().any(|task| task == "release:pack"),
        "the artifact task must not run after a hosted wait: {:?}",
        effects.tasks
    );
    assert!(
        remote_tags(&waited).is_empty(),
        "nothing publishes while waiting"
    );

    // A resume re-checks the same gate and continues with the evidence now in
    // hand. The gate's `exit 7` command never runs.
    effects.hosted_evidence = vec![HostedEvidence::Accepted {
        repository: "acme/demo".to_owned(),
        commit: String::new(),
        run: crate::hosted_evidence::BoundRun {
            run_id: Some(1),
            url: "https://github.com/acme/demo/actions/runs/1".to_owned(),
        },
    }];
    let resumed = resume(&waited, &mut effects);
    assert!(
        resumed.succeeded(),
        "resume failed: {:?}",
        resumed.record.errors
    );
    assert_eq!(resumed.record.status, ReleaseRunStatus::Complete);
    let hosted = resumed
        .record
        .gates
        .iter()
        .find(|gate| gate.name == "hosted")
        .expect("hosted gate");
    assert!(hosted.passed);
    assert_eq!(
        hosted.hosted_run.as_deref(),
        Some("https://github.com/acme/demo/actions/runs/1")
    );
    // The candidate commit is the commit the evidence was asked about.
    let candidate = resumed.record.candidate_commit.clone().expect("candidate");
    assert!(effects.tasks.iter().all(|task| task != "exit 7"));
    assert_eq!(remote_tags(&waited).get("v0.2.5"), Some(&candidate));

    // Failed evidence fails the gate closed instead of waiting.
    let failed_fixture = fixture("hosted-failed");
    single_identity_project(&failed_fixture);
    append_manifest(
        &failed_fixture,
        "[release.gates.hosted]\ncommand = \"exit 7\"\nreuse-hosted-evidence = true\nstage = \"certify\"\n\n[release.hosted-evidence]\nworkflow = \"ci.yml\"\n",
    );
    commit_and_push(&failed_fixture);
    let mut effects = TestEffects::new(&failed_fixture.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());
    effects.hosted_evidence = vec![HostedEvidence::Failed {
        detail: "hosted evidence for gate `hosted` failed: run … conclusion is `failure`."
            .to_owned(),
        run: None,
    }];
    let outcome = run(&failed_fixture, &mut effects, ReleaseAction::Run, None);
    assert_eq!(outcome.record.status, ReleaseRunStatus::Failed);
    assert!(
        outcome
            .record
            .errors
            .iter()
            .any(|error| error.contains("hosted evidence")),
        "{:?}",
        outcome.record.errors
    );
    assert!(remote_tags(&failed_fixture).is_empty());
}

#[test]
fn a_failed_task_target_verify_is_published_but_unverified() {
    let fixture = fixture("task-unverified");
    single_identity_project(&fixture);
    append_manifest(&fixture, NPM_TARGET);
    commit_and_push(&fixture);

    let mut effects = TestEffects::new(&fixture.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());
    effects.task_outputs.insert(
        "release:publish:npm".to_owned(),
        npm_output("true", &["fixture@0.2.5"]),
    );
    effects.fail_task = Some("release:verify:npm".to_owned());

    let paused = run(&fixture, &mut effects, ReleaseAction::Run, None);
    let outcome = approve_publish(&fixture, &mut effects, &paused);

    assert_eq!(outcome.record.status, ReleaseRunStatus::Complete);
    let receipt = outcome.receipt();
    assert!(receipt.published, "the release stays published");
    assert!(receipt.unverified, "the failure is recorded, not undone");
    assert!(receipt
        .errors
        .iter()
        .any(|error| error.contains("published but unverified")));
    assert_eq!(
        outcome.record.target("npm").expect("npm").state,
        RecordTargetState::Published
    );
    // Never unpublished: the destination is still recorded.
    assert_eq!(
        outcome
            .record
            .target("npm")
            .expect("npm")
            .destinations
            .len(),
        1
    );
}

#[test]
fn a_hosted_wait_binds_the_dispatched_run_and_a_resume_rechecks_it() {
    let hosted = fixture("hosted-bind");
    single_identity_project(&hosted);
    append_manifest(
        &hosted,
        "[release.gates.hosted]\ncommand = \"exit 7\"\nreuse-hosted-evidence = true\nstage = \"certify\"\n\n[release.hosted-evidence]\nworkflow = \"ci.yml\"\n",
    );
    commit_and_push(&hosted);

    let mut effects = TestEffects::new(&hosted.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());
    // The release dispatches once and the run it started is still going: the
    // wait binds that run instead of waiting for a newer one.
    effects.hosted_evidence = vec![HostedEvidence::Waiting {
        detail: "hosted evidence for gate `hosted` is pending: run 7 status is `in_progress`."
            .to_owned(),
        run: Some(crate::hosted_evidence::BoundRun {
            run_id: Some(7),
            url: "https://github.com/acme/demo/actions/runs/7".to_owned(),
        }),
    }];
    let waiting = run(&hosted, &mut effects, ReleaseAction::Run, None);
    assert!(
        waiting.succeeded(),
        "a hosted wait is not a failure: {:?}",
        waiting.record.errors
    );
    assert_eq!(
        waiting.record.status,
        ReleaseRunStatus::AwaitingHostedEvidence
    );
    // The Run records the dispatch window it opened and the run it bound, so
    // the receipt names one run, not whichever run is newest.
    let window: &super::record::RecordHostedWait =
        waiting.record.hosted_wait("hosted").expect("hosted wait");
    assert_eq!(window.workflow, "ci.yml");
    assert!(!window.created_after.is_empty());
    assert_eq!(window.run_id, Some(7));
    assert_eq!(
        window.run_url.as_deref(),
        Some("https://github.com/acme/demo/actions/runs/7")
    );
    assert_eq!(effects.hosted_evidence_bindings.len(), 1);
    assert_eq!(
        effects.hosted_evidence_bindings[0].created_after.as_deref(),
        Some(window.created_after.as_str())
    );
    assert_eq!(effects.hosted_evidence_bindings[0].run_id, None);

    // A resume re-checks the bound run: the same window, and the run id the
    // record journalled.
    effects.hosted_evidence = vec![HostedEvidence::Accepted {
        repository: "acme/demo".to_owned(),
        commit: String::new(),
        run: crate::hosted_evidence::BoundRun {
            run_id: Some(7),
            url: "https://github.com/acme/demo/actions/runs/7".to_owned(),
        },
    }];
    let resumed = resume(&hosted, &mut effects);
    assert!(
        resumed.succeeded(),
        "resume failed: {:?}",
        resumed.record.errors
    );
    assert_eq!(effects.hosted_evidence_bindings.len(), 2);
    assert_eq!(
        effects.hosted_evidence_bindings[1].created_after.as_deref(),
        Some(window.created_after.as_str()),
        "a resume keeps the dispatch window it opened"
    );
    assert_eq!(
        effects.hosted_evidence_bindings[1].run_id,
        Some(7),
        "a resume re-checks the bound run, never a sibling"
    );
    let gate = resumed
        .record
        .gates
        .iter()
        .find(|gate| gate.name == "hosted")
        .expect("hosted gate");
    assert!(gate.passed);
    assert_eq!(gate.hosted_run_id, Some(7));
    assert_eq!(
        resumed.record.hosted_wait("hosted").expect("wait").run_id,
        Some(7)
    );
}

#[test]
fn an_ambiguous_hosted_answer_fails_the_gate_and_binds_nothing() {
    let hosted = fixture("hosted-ambiguous");
    single_identity_project(&hosted);
    append_manifest(
        &hosted,
        "[release.gates.hosted]\ncommand = \"exit 7\"\nreuse-hosted-evidence = true\nstage = \"certify\"\n\n[release.hosted-evidence]\nworkflow = \"ci.yml\"\n",
    );
    commit_and_push(&hosted);

    let mut effects = TestEffects::new(&hosted.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());
    effects.hosted_evidence = vec![HostedEvidence::Failed {
        detail: "hosted evidence for gate `hosted` is ambiguous: 2 runs match workflow `ci.yml` for commit `abc123`; re-run when only its own dispatch is live".to_owned(),
        run: None,
    }];
    let outcome = run(&hosted, &mut effects, ReleaseAction::Run, None);
    assert_eq!(outcome.record.status, ReleaseRunStatus::Failed);
    assert!(
        outcome
            .record
            .errors
            .iter()
            .any(|error| error.contains("is ambiguous")),
        "{:?}",
        outcome.record.errors
    );
    let gate = outcome
        .record
        .gates
        .iter()
        .find(|gate| gate.name == "hosted")
        .expect("hosted gate");
    assert!(!gate.passed);
    assert_eq!(gate.hosted_run_id, None);
    assert_eq!(
        outcome.record.hosted_wait("hosted").expect("wait").run_id,
        None
    );
    assert!(remote_tags(&hosted).is_empty());
}

#[test]
fn long_gate_gets_fresh_settlement_budget() {
    use effigy_kernel::{
        ctx::*,
        observe::Observed,
        root::StateRoot,
        run::{self, EffectState, Phase},
        store::Store,
    };
    let fixture = fixture("long-gate-budget");
    single_identity_project(&fixture);
    write(
        &fixture.root,
        "gates/pre.sh",
        "#!/bin/sh\nsleep 1\necho pre\n",
    );
    commit_all(&fixture.root, "slow gate");
    git(&fixture.root, &["push"]);
    let mut effects = TestEffects::new(&fixture.root);
    let outcome = run_release_with_budget(
        &request(&fixture, ReleaseAction::Run, Some(ReleaseStage::PreGates)),
        &mut effects,
        &mut SilentGateObserver,
        std::time::Duration::from_millis(500),
    )
    .expect("long gate settles");
    assert!(outcome.succeeded());
    let op = OperationContext::new(
        Deadline::At(std::time::Instant::now() + std::time::Duration::from_secs(30)),
        CancelToken::default(),
        Limits {
            capture_tail: 65536,
            log_rotate: 8 << 20,
            log_keep: 2,
            line_max: 65536,
            queue_bytes: 1 << 20,
            probe_concurrency: 2,
        },
        EffectClass::Observe,
        Placement::Host,
    );
    let observe = op.observe().unwrap();
    let location = effigy_kernel::location::CheckoutLocation {
        root: fixture.root.clone(),
    };
    let Observed::Verified(root, _) =
        StateRoot::checkout_observe_in(&observe, &fixture.home, &location)
    else {
        panic!("state root")
    };
    let Observed::Verified(reader, _) = Store::read(&observe, &root) else {
        panic!("store")
    };
    let id = outcome.record.run_id.parse().unwrap();
    let Observed::Verified(summary, _) = run::summary(&reader, id) else {
        panic!("summary")
    };
    assert_eq!(summary.phase, Phase::Closed);
    assert_eq!(summary.cleanup_complete, Some(true));
    let Observed::Verified(effects, _) = run::effects(&reader, id) else {
        panic!("effects")
    };
    assert!(effects
        .iter()
        .all(|effect| effect.state != EffectState::Applied));
}

#[test]
fn interrupted_release_settles_without_advancing_stages() {
    let fixture = fixture("settle-only");
    single_identity_project(&fixture);
    let mut effects = TestEffects::new(&fixture.root);
    let paused = run(
        &fixture,
        &mut effects,
        ReleaseAction::Run,
        Some(ReleaseStage::PreGates),
    );
    let session = ReleaseSession::open(&fixture.home, &fixture.root).unwrap();
    let state = session.state().unwrap();
    let id = new_run_id().unwrap();
    let mut run = session.begin(id, &mut ReportsOnly).unwrap();
    let mut record = paused.record.clone();
    record.run_id = id.to_string();
    state.write_record(&record, &mut run).unwrap();
    state.write_receipt(&record, &mut run).unwrap();
    run.set_engine_record(&serde_json::to_value(record.checkpoint(&state.record_path)).unwrap())
        .unwrap();
    drop(run);

    // Pending settlement is a journal observation. The recorded stage outcome
    // and both receipt surfaces stay as the Run recorded them.
    let before = read_release_observation(&fixture.home, &fixture.root)
        .unwrap()
        .expect("observation");
    assert_eq!(before.settlement, ReleaseSettlement::Interrupted);
    assert_eq!(before.record.status, ReleaseRunStatus::Paused);
    assert!(!before
        .record
        .errors
        .iter()
        .any(|e| e.contains("settlement")));
    assert_receipt_file_matches_command(&paused.receipt_path, &before.record);

    let settled = settle_release(&fixture.home, &fixture.root, &record.release_id).unwrap();
    assert_eq!(settled.run_id, id.to_string());
    assert_eq!(settled.stage, record.stage);
    assert_eq!(settled.status, ReleaseRunStatus::Paused);
    assert!(!settled.errors.iter().any(|e| e.contains("settlement")));
    let after = read_release_observation(&fixture.home, &fixture.root)
        .unwrap()
        .expect("observation");
    assert_eq!(after.settlement, ReleaseSettlement::Settled);
    assert_eq!(
        ReleaseReceipt::from_record(&settled),
        ReleaseReceipt::from_record(&after.record)
    );
    assert_receipt_file_matches_command(&paused.receipt_path, &after.record);
    assert!(!ReleaseReceipt::from_record(&settled).published);
    assert!(effects.pushes.is_empty());
}

#[test]
fn a_complete_run_with_pending_settlement_keeps_its_complete_receipt() {
    let fixture = fixture("settle-after-closeout");
    single_identity_project(&fixture);
    let mut effects = TestEffects::new(&fixture.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());
    let complete = run(&fixture, &mut effects, ReleaseAction::Run, None);
    assert!(
        complete.succeeded(),
        "run failed: {:?}",
        complete.record.errors
    );
    let pushes = effects.commit_pushes.clone();

    // Interruption after the final durable checkpoint: the Run never settles.
    let session = ReleaseSession::open(&fixture.home, &fixture.root).unwrap();
    let state = session.state().unwrap();
    let id = new_run_id().unwrap();
    let mut run = session.begin(id, &mut ReportsOnly).unwrap();
    let mut record = complete.record.clone();
    record.run_id = id.to_string();
    state.write_record(&record, &mut run).unwrap();
    state.write_receipt(&record, &mut run).unwrap();
    run.set_engine_record(&serde_json::to_value(record.checkpoint(&state.record_path)).unwrap())
        .unwrap();
    drop(run);

    let pending = read_release_observation(&fixture.home, &fixture.root)
        .unwrap()
        .expect("observation");
    assert_eq!(pending.settlement, ReleaseSettlement::Interrupted);
    assert_eq!(pending.record.status, ReleaseRunStatus::Complete);
    let file = assert_receipt_file_matches_command(&complete.receipt_path, &pending.record);
    assert_eq!(
        file["release"]["status"],
        ReleaseRunStatus::Complete.label()
    );

    // Settling adds no stage, publication or push.
    let settled = settle_release(&fixture.home, &fixture.root, &record.release_id).unwrap();
    assert_eq!(settled.status, ReleaseRunStatus::Complete);
    let after = read_release_observation(&fixture.home, &fixture.root)
        .unwrap()
        .expect("observation");
    assert_eq!(after.settlement, ReleaseSettlement::Settled);
    assert_receipt_file_matches_command(&complete.receipt_path, &after.record);
    assert_eq!(effects.commit_pushes, pushes);
}

#[test]
fn process_cleanup_budget_starts_after_child_exit() {
    let fixture = fixture("process-cleanup-budget");
    single_identity_project(&fixture);
    let session = ReleaseSession::open(&fixture.home, &fixture.root).unwrap();
    let mut run = session
        .begin(new_run_id().unwrap(), &mut ReportsOnly)
        .unwrap();
    let effects = GitEffects::with_executable(&fixture.root, custody_fixture());
    let output = effects
        .execute_with_cleanup_budget(
            &mut run,
            vec!["/bin/sh".into(), "-c".into(), "sleep 1".into()],
            &fixture.root,
            &BTreeMap::new(),
            "slow fixture child",
            std::time::Duration::from_millis(500),
        )
        .unwrap();
    assert!(output.status.success());
    run.finish(WorkloadOutcome::Succeeded).unwrap();
    assert_eq!(
        run.settle(&session.cleanup().unwrap(), &mut ReportsOnly)
            .unwrap()
            .cleanup(),
        effigy_kernel::outcome::CleanupOutcome::Complete
    );
}

/// A `release decide` on a target that already records an outcome names that
/// outcome in words. The `Debug` variant name it used to print never reaches
/// the operator (effigy#402, Q-017).
#[test]
fn deciding_a_target_that_already_has_an_outcome_reads_it_in_words() {
    let rendered = super::not_an_unknown_publish("npm", RecordTargetState::Published).to_string();
    assert!(
        rendered.contains("release target `npm` records outcome `published`"),
        "{rendered}"
    );
    assert!(rendered.contains("rather than `unknown`"), "{rendered}");
    assert!(
        !rendered.contains("Published"),
        "no variant name reaches a person: {rendered}"
    );
    for state in [
        RecordTargetState::Pending,
        RecordTargetState::Unknown,
        RecordTargetState::Published,
        RecordTargetState::Failed,
    ] {
        let label = state.label();
        assert!(
            label.chars().all(|c| c.is_ascii_lowercase()),
            "a target state label reads as prose: {label}"
        );
    }
}
