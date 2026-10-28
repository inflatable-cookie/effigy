mod gate_reports;
mod git;
mod hosted_evidence;
mod model;
mod pipeline;
mod prepare_helpers;
mod render_json;
mod run;
#[cfg(test)]
pub mod test_support;
mod text;
mod version;

pub use hosted_evidence::{
    collect_hosted_evidence, evaluate_hosted_runs, github_repository_from_remote_url,
    parse_gh_run_list_json, GhCliHostedEvidenceLookup, HostedEvidenceError, HostedEvidenceLookup,
    HostedEvidenceQuery, HostedEvidenceSpec, HostedRun,
};
pub use model::{
    FileMutationApply, FileMutationPlan, GateExecutionReport, GateResult, ReleaseConfig,
    ReleaseContext, ReleaseError, ReleaseGateRun, ReleaseStatus,
};
pub use pipeline::{
    plan_release_pipeline, resolve_release_pipeline, GateStage, IdentityKind, ReleaseArtifactPlan,
    ReleaseIdentityPlan, ReleasePipeline, ReleasePipelinePlan, ReleaseRunSummary, ResolvedArtifact,
    ResolvedIdentity, ResolvedTarget, TargetKind, RELEASE_STAGES,
};
use prepare_helpers::{apply_bump, unreleased_counts};
pub use prepare_helpers::{
    apply_release_mutations, gate_blockers, gate_blockers_if_checked,
    render_prepared_changelog_contents, restore_mutation_snapshots, snapshot_mutation_paths,
    suggested_bump,
};
pub use run::{
    new_run_id, parse_release_task_output, read_release_observation_for_root, read_release_record,
    read_release_task_output, release_home, release_input_value, release_owner,
    render_release_receipt_json, render_release_receipt_text, render_release_run_json, run_release,
    run_release_with_observer, settle_release, sha256_bytes, verify_artifact_digests, GitEffects,
    HostedEvidence, RecordApproval, RecordArtifact, RecordArtifactFile, RecordCheckpoint,
    RecordDecision, RecordDestination, RecordGate, RecordIdentity, RecordTarget, RecordTargetState,
    RecordVerification, ReleaseAction, ReleaseCheckpoint, ReleaseEffects, ReleaseReceipt,
    ReleaseRecord, ReleaseRunOutcome, ReleaseRunRequest, ReleaseRunStatus, ReleaseSession,
    ReleaseStage, ReleaseState, ReleaseTaskOutput, ReleaseTaskSubject, TargetDecision, TaskRequest,
    TaskResponse, RELEASE_INPUT_ENV, RELEASE_OUTPUT_ENV, RELEASE_RECEIPT_SCHEMA,
    RELEASE_RECORD_SCHEMA, RELEASE_TASK_INPUT_SCHEMA, RELEASE_TASK_OUTPUT_SCHEMA,
};

pub use effigy_changelog::BumpKind;
pub use git::{
    git_add_release_files, git_commit_release, git_create_tag, git_current_branch, git_head_sha,
    git_modified_files, git_push_release, git_remote_url, git_tag_exists, git_work_tree_clean,
};
pub use render_json::{
    render_release_gate_run_json, render_release_plan_json, render_release_status_json,
};
pub use text::{
    render_release_gate_run_text, render_release_pipeline_plan_text, render_release_status_text,
};
pub use version::{
    build_changelog_mutation_detail_lines, build_diff_preview, build_version_mutation_detail_lines,
    detect_cargo_version_path, detect_pyproject_version_path, detect_version_file_kind,
    json_value_at_path, read_current_version, render_changelog_preview_line,
    render_updated_version_contents, render_version_preview_line,
    replace_json_string_at_path_preserving_layout, resolve_version_field_path, toml_value_at_path,
};

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::time::Instant;

use effigy_changelog::{self as changelog};
use effigy_manifest::config_sections::ManifestReleaseConfig;
use effigy_manifest::load_task_manifest;
use hosted_evidence::hosted_evidence_spec_from_manifest;

use effigy_manifest::TASK_MANIFEST_FILE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionFileKind {
    CargoToml,
    PackageJson,
    PyProjectToml,
    PlainText,
}

impl VersionFileKind {
    pub fn format_label(self) -> &'static str {
        match self {
            VersionFileKind::CargoToml => "cargo.toml",
            VersionFileKind::PackageJson => "package.json",
            VersionFileKind::PyProjectToml => "pyproject.toml",
            VersionFileKind::PlainText => "plain-text",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedVersionSource {
    pub path: PathBuf,
    pub kind: VersionFileKind,
    pub field_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedGate {
    pub name: String,
    /// The shell command a gate runs. A `task` gate's command is `effigy
    /// <task>`, so one runner executes both forms.
    pub command: String,
    /// The Effigy task a `task` gate names, when it was declared that way.
    pub task: Option<String>,
    pub description: Option<String>,
    /// Contract 054 §4.2: `pre` runs before any version edit, `certify` after.
    pub stage: pipeline::GateStage,
    pub reuse_hosted_evidence: bool,
    pub hosted_evidence: Option<HostedEvidenceSpec>,
}

#[derive(Debug, Clone)]
pub struct ResolvedSyncFile {
    pub path: PathBuf,
    pub kind: SyncFileKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncFileKind {
    CargoLock,
    PackageJson,
}

/// Blockers describing a rollback that could not complete.
///
/// Empty on the normal path, which is the point: a failed prepare says nothing
/// extra when it has successfully put the tree back, and says exactly which
/// files it could not restore when it has not.
pub fn load_release_config(root: &Path) -> Result<ReleaseConfig, ReleaseError> {
    let manifest_path = root.join(TASK_MANIFEST_FILE);
    let manifest = if manifest_path.exists() {
        Some(load_task_manifest(&manifest_path)?)
    } else {
        None
    };
    let manifest_release = manifest.as_ref().and_then(|parsed| parsed.release.as_ref());
    reject_pipeline_release_for_single_identity_commands(manifest_release)?;
    let version_source = resolve_version_source(root, manifest_release)?;
    let changelog_path = resolve_config_path(
        root,
        manifest_release.and_then(|config| config.changelog.as_deref()),
        "CHANGELOG.md",
        "release.changelog",
    )?;
    if !changelog_path.exists() {
        return Err(ReleaseError::TaskInvocation(format!(
            "release changelog path does not exist: {}",
            changelog_path.display()
        )));
    }

    let gates = manifest_release
        .map(resolve_gates)
        .transpose()?
        .unwrap_or_default();
    validate_sync_files(manifest_release)?;
    let sync_files = resolve_sync_files(root, manifest_release, &version_source)?;
    let tag_format = manifest_release
        .and_then(|config| config.tag_format.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("v{version}")
        .to_owned();
    if !tag_format.contains("{version}") {
        return Err(ReleaseError::TaskInvocation(
            "release.tag-format must contain `{version}`".to_owned(),
        ));
    }

    Ok(ReleaseConfig {
        version_source,
        changelog_path,
        pre_1_0: manifest_release
            .and_then(|config| config.pre_1_0)
            .unwrap_or(true),
        initial_tag_current_version: manifest_release
            .and_then(|config| config.initial_tag_current_version)
            .unwrap_or(false),
        sync_files,
        gates,
        tag_format,
        hosted_evidence: hosted_evidence_spec_from_manifest(
            manifest_release.and_then(|config| config.hosted_evidence.as_ref()),
        )?,
    })
}

/// `release prepare` and `release execute` still run the single-identity
/// shorthand. A release that declares named identities, artifacts, targets or
/// approval checkpoints resolves in `release status` and `release --plan` today
/// and runs through the Run pipeline; refusing here keeps `prepare` from
/// bumping one version file of a many-identity release.
fn reject_pipeline_release_for_single_identity_commands(
    config: Option<&ManifestReleaseConfig>,
) -> Result<(), ReleaseError> {
    let Some(config) = config else {
        return Ok(());
    };
    if !config.identities.is_empty() {
        return Err(ReleaseError::TaskInvocation(format!(
            "`release prepare` and `release execute` still run the single-identity shorthand, and this release declares named identities ({}); `effigy release status` and `effigy release --plan` resolve them today, and `release run` runs them once the Run pipeline lands (contract 054 §5)",
            config
                .identities
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    if !config.artifacts.is_empty() || !config.targets.is_empty() || config.approve.is_some() {
        return Err(ReleaseError::TaskInvocation(
            "`release prepare` and `release execute` still run the single-identity shorthand, which has one `tag` target, no artifacts and no approval checkpoints; `effigy release status` and `effigy release --plan` resolve `release.artifacts`, `release.targets` and `release.approve` today, and `release run` runs them once the Run pipeline lands (contract 054 §5)".to_owned(),
        ));
    }
    Ok(())
}

pub fn validate_sync_files(config: Option<&ManifestReleaseConfig>) -> Result<(), ReleaseError> {
    let Some(config) = config else {
        return Ok(());
    };
    for path in &config.sync_files {
        if path.trim().is_empty() {
            return Err(ReleaseError::TaskInvocation(
                "release.sync-files entries must not be empty".to_owned(),
            ));
        }
    }
    Ok(())
}

pub fn resolve_version_source(
    root: &Path,
    config: Option<&ManifestReleaseConfig>,
) -> Result<ResolvedVersionSource, ReleaseError> {
    if let Some(configured_path) = config.and_then(|config| config.version_file.as_deref()) {
        let trimmed = configured_path.trim();
        if trimmed.is_empty() {
            return Err(ReleaseError::TaskInvocation(
                "release.version-file must not be empty".to_owned(),
            ));
        }
        let path = root.join(trimmed);
        if !path.exists() {
            return Err(ReleaseError::TaskInvocation(format!(
                "release version file does not exist: {}",
                path.display()
            )));
        }
        let kind = detect_version_file_kind(&path).ok_or_else(|| {
            ReleaseError::TaskInvocation(format!(
                "unsupported release version file: {}",
                path.display()
            ))
        })?;
        let field_path = resolve_resolved_version_field_path(
            &path,
            kind,
            config.and_then(|value| value.version_path.as_deref()),
        )?;
        return Ok(ResolvedVersionSource {
            path,
            kind,
            field_path,
        });
    }

    for candidate in ["Cargo.toml", "package.json", "pyproject.toml", "VERSION"] {
        let path = root.join(candidate);
        if !path.exists() {
            continue;
        }
        let kind = detect_version_file_kind(&path).ok_or_else(|| {
            ReleaseError::TaskInvocation(format!(
                "unsupported release version file: {}",
                path.display()
            ))
        })?;
        let field_path = resolve_resolved_version_field_path(&path, kind, None)?;
        return Ok(ResolvedVersionSource {
            path,
            kind,
            field_path,
        });
    }

    Err(ReleaseError::TaskInvocation(
        "no release version file found; configure [release].version-file or add Cargo.toml, package.json, pyproject.toml, or VERSION at the repo root".to_owned(),
    ))
}

fn resolve_resolved_version_field_path(
    path: &Path,
    kind: VersionFileKind,
    configured: Option<&str>,
) -> Result<Option<String>, ReleaseError> {
    if configured.is_some() {
        return resolve_version_field_path(kind, configured);
    }

    match kind {
        VersionFileKind::CargoToml => {
            let raw = std::fs::read_to_string(path).map_err(|error| {
                ReleaseError::TaskInvocation(format!(
                    "failed to read release version file {}: {error}",
                    path.display()
                ))
            })?;
            let parsed = toml::from_str::<toml::Value>(&raw).map_err(|error| {
                ReleaseError::TaskInvocation(format!("failed to parse {}: {error}", path.display()))
            })?;
            detect_cargo_version_path(&parsed)
                .map(|value| Some(value.to_owned()))
                .ok_or_else(|| {
                    ReleaseError::TaskInvocation(format!(
                        "could not find version field in {} (tried `package.version` and `workspace.package.version` via `package.version.workspace = true`)",
                        path.display()
                    ))
                })
        }
        _ => resolve_version_field_path(kind, None),
    }
}

pub fn resolve_config_path(
    root: &Path,
    configured: Option<&str>,
    default_name: &str,
    field: &str,
) -> Result<PathBuf, ReleaseError> {
    if let Some(configured) = configured {
        let trimmed = configured.trim();
        if trimmed.is_empty() {
            return Err(ReleaseError::TaskInvocation(format!(
                "{field} must not be empty"
            )));
        }
        return Ok(root.join(trimmed));
    }
    Ok(root.join(default_name))
}

pub fn resolve_gates(config: &ManifestReleaseConfig) -> Result<Vec<ResolvedGate>, ReleaseError> {
    pipeline::resolve_gates(Some(config))
}

pub fn resolve_sync_files(
    root: &Path,
    config: Option<&ManifestReleaseConfig>,
    version_source: &ResolvedVersionSource,
) -> Result<Vec<ResolvedSyncFile>, ReleaseError> {
    let Some(config) = config else {
        return Ok(Vec::new());
    };

    let mut resolved = Vec::new();
    let mut seen = BTreeSet::new();
    for configured in &config.sync_files {
        let trimmed = configured.trim();
        let path = resolve_sync_file_path(root, trimmed)?;
        if !seen.insert(path.clone()) {
            continue;
        }
        match path.file_name().and_then(|name| name.to_str()) {
            Some("Cargo.lock") if matches!(version_source.kind, VersionFileKind::CargoToml) => {
                require_cargo_workspace_root(&path)?;
                resolved.push(ResolvedSyncFile {
                    path,
                    kind: SyncFileKind::CargoLock,
                });
            }
            Some("Cargo.lock") => {
                return Err(ReleaseError::TaskInvocation(
                    "release.sync-files `Cargo.lock` is only supported when the release version file is Cargo.toml".to_owned(),
                ));
            }
            Some("package.json") if path == version_source.path => {
                return Err(ReleaseError::TaskInvocation(
                    "release.sync-files must not repeat the primary release version file"
                        .to_owned(),
                ));
            }
            Some("package.json") => {
                let source = ResolvedVersionSource {
                    path: path.clone(),
                    kind: VersionFileKind::PackageJson,
                    field_path: Some("version".to_owned()),
                };
                read_current_version(&source)?;
                resolved.push(ResolvedSyncFile {
                    path,
                    kind: SyncFileKind::PackageJson,
                });
            }
            Some(other) => {
                return Err(ReleaseError::TaskInvocation(format!(
                    "unsupported release.sync-files entry `{other}`; supported entries are `Cargo.lock` and `package.json`"
                )));
            }
            None => {
                return Err(ReleaseError::TaskInvocation(
                    "release.sync-files entries must resolve to a file path".to_owned(),
                ));
            }
        }
    }

    Ok(resolved)
}

/// Resolve one declared `release.sync-files` entry inside the release root.
///
/// A sync file is a repo-relative, declared path. Absolute paths, `..`
/// components and any path whose resolved location leaves the release root are
/// refused, so the prepare stage can only rewrite files the release root owns.
/// A nested entry stays allowed: contract 054 §4.1 syncs the lockfile of a
/// workspace-excluded Cargo consumer, which sits below the root on purpose.
fn resolve_sync_file_path(root: &Path, configured: &str) -> Result<PathBuf, ReleaseError> {
    let raw = Path::new(configured);
    if raw.is_absolute() {
        return Err(ReleaseError::TaskInvocation(format!(
            "release.sync-files entry `{configured}` must be relative to the release root, not an absolute path"
        )));
    }
    let mut relative = PathBuf::new();
    for component in raw.components() {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(ReleaseError::TaskInvocation(format!(
                    "release.sync-files entry `{configured}` must not contain `..`; a sync file stays inside the release root"
                )));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(ReleaseError::TaskInvocation(format!(
                    "release.sync-files entry `{configured}` must be relative to the release root"
                )));
            }
        }
    }
    if relative.as_os_str().is_empty() {
        return Err(ReleaseError::TaskInvocation(
            "release.sync-files entries must name a file".to_owned(),
        ));
    }
    let path = root.join(&relative);
    ensure_sync_path_inside_root(root, &path, configured)?;
    Ok(path)
}

/// Refuse a sync path whose resolved location leaves the release root.
///
/// Canonicalising the nearest existing ancestor catches a symlinked directory
/// that points outside the checkout, so a declared path cannot reach another
/// workspace by alias.
fn ensure_sync_path_inside_root(
    root: &Path,
    path: &Path,
    configured: &str,
) -> Result<(), ReleaseError> {
    let canonical_root = std::fs::canonicalize(root).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to resolve the release root {} for release.sync-files: {error}",
            root.display()
        ))
    })?;
    let mut probe = path.to_path_buf();
    loop {
        match std::fs::canonicalize(&probe) {
            Ok(resolved) => {
                if resolved.starts_with(&canonical_root) {
                    return Ok(());
                }
                return Err(ReleaseError::TaskInvocation(format!(
                    "release.sync-files entry `{configured}` resolves to {} outside the release root {}",
                    resolved.display(),
                    canonical_root.display()
                )));
            }
            Err(_) => match probe.parent() {
                Some(parent) if parent != probe => probe = parent.to_path_buf(),
                _ => {
                    return Err(ReleaseError::TaskInvocation(format!(
                        "release.sync-files entry `{configured}` could not be resolved inside the release root"
                    )));
                }
            },
        }
    }
}

/// A synced `Cargo.lock` must be the lockfile of a Cargo workspace rooted at
/// its own directory.
///
/// A lockfile under a plain member directory belongs to the enclosing
/// workspace, so syncing it would rewrite a lockfile the release did not
/// declare. Requiring a sibling `Cargo.toml` states that ownership up front.
fn require_cargo_workspace_root(lockfile: &Path) -> Result<(), ReleaseError> {
    let Some(directory) = lockfile.parent() else {
        return Err(ReleaseError::TaskInvocation(format!(
            "release.sync-files entry `{}` must name a Cargo.lock file inside the release root",
            lockfile.display()
        )));
    };
    if !directory.join("Cargo.toml").is_file() {
        return Err(ReleaseError::TaskInvocation(format!(
            "release.sync-files entry `{}` has no Cargo.toml beside it; declare the Cargo.lock of a workspace rooted at its own directory"
,
            lockfile.display()
        )));
    }
    Ok(())
}

fn format_duration_ms(duration_ms: u128) -> String {
    if duration_ms >= 1_000 {
        format!("{:.1}s", duration_ms as f64 / 1_000.0)
    } else {
        format!("{duration_ms}ms")
    }
}

pub fn format_release_tag(tag_format: &str, version: &semver::Version) -> String {
    tag_format.replace("{version}", &version.to_string())
}

pub fn run_release_gates(
    root: &Path,
    gates: &[ResolvedGate],
    fail_fast: bool,
) -> GateExecutionReport {
    run_release_gates_with_lookup(root, gates, fail_fast, &GhCliHostedEvidenceLookup, |_| {})
}

pub fn run_release_gates_with_progress<F>(
    root: &Path,
    gates: &[ResolvedGate],
    fail_fast: bool,
    progress: F,
) -> GateExecutionReport
where
    F: FnMut(&str),
{
    run_release_gates_with_lookup(root, gates, fail_fast, &GhCliHostedEvidenceLookup, progress)
}

pub fn run_release_gates_with_lookup<F>(
    root: &Path,
    gates: &[ResolvedGate],
    fail_fast: bool,
    lookup: &dyn HostedEvidenceLookup,
    progress: F,
) -> GateExecutionReport
where
    F: FnMut(&str),
{
    let mut observer = TextGateObserver { progress };
    run_release_gates_observed(root, gates, fail_fast, lookup, &mut observer)
}

/// How a gate run says what it is doing, as it does it.
///
/// A gate can take minutes, so the command plane draws each one as a live
/// step (`running gate cargo test (2/5)`) instead of waiting for the end. The
/// engine reports the facts; the plane decides how they look, which is why
/// `--json` and piped output stay silent.
pub trait GateObserver {
    /// A gate is about to run, or about to reuse hosted evidence.
    fn gate_started(
        &mut self,
        position: usize,
        total: usize,
        gate: &ResolvedGate,
        reusing_hosted_evidence: bool,
    ) {
        let _ = (position, total, gate, reusing_hosted_evidence);
    }
    /// The gate finished, passed or failed.
    fn gate_finished(&mut self, position: usize, total: usize, result: &GateResult) {
        let _ = (position, total, result);
    }
    /// A release Run moved on to `stage`. A one-off gate run has no stages.
    fn stage(&mut self, stage: &str) {
        let _ = stage;
    }
    /// Progress that is not one gate: hosted evidence lookup, for instance.
    fn note(&mut self, message: &str) {
        let _ = message;
    }
}

/// An observer that draws nothing: for callers that only want the result.
pub struct SilentGateObserver;

impl GateObserver for SilentGateObserver {}

/// The text observer: exactly the lines #290 streamed, for callers that draw
/// plain lines rather than steps.
struct TextGateObserver<F: FnMut(&str)> {
    progress: F,
}

impl<F: FnMut(&str)> GateObserver for TextGateObserver<F> {
    fn gate_started(
        &mut self,
        position: usize,
        total: usize,
        gate: &ResolvedGate,
        reusing_hosted_evidence: bool,
    ) {
        if reusing_hosted_evidence {
            (self.progress)(&format!(
                "gate {position}/{total} `{}` ({}) reusing hosted evidence",
                gate.name,
                gate.stage.stage_name()
            ));
        } else {
            (self.progress)(&format!(
                "gate {position}/{total} `{}` ({}) starting: {}",
                gate.name,
                gate.stage.stage_name(),
                gate.command
            ));
        }
    }

    fn gate_finished(&mut self, position: usize, total: usize, result: &GateResult) {
        (self.progress)(&format!(
            "gate {position}/{total} `{}` {} ({})",
            result.name,
            gate_outcome_word(result),
            format_duration_ms(result.duration_ms),
        ));
    }

    fn note(&mut self, message: &str) {
        (self.progress)(message);
    }
}

/// The one word a finished gate reads as.
pub fn gate_outcome_word(result: &GateResult) -> &'static str {
    if result.reused {
        "reused"
    } else if result.passed {
        "passed"
    } else {
        "failed"
    }
}

/// Run gates, reporting each start and finish to `observer`.
pub fn run_release_gates_with_observer(
    root: &Path,
    gates: &[ResolvedGate],
    fail_fast: bool,
    observer: &mut dyn GateObserver,
) -> GateExecutionReport {
    run_release_gates_observed(root, gates, fail_fast, &GhCliHostedEvidenceLookup, observer)
}

fn run_release_gates_observed(
    root: &Path,
    gates: &[ResolvedGate],
    fail_fast: bool,
    lookup: &dyn HostedEvidenceLookup,
    observer: &mut dyn GateObserver,
) -> GateExecutionReport {
    let started = Instant::now();
    let environment_path = gate_reports::persist_gate_run_environment(root);
    let mut results = Vec::with_capacity(gates.len());
    let mut stopped_early = false;
    let hosted = resolve_run_hosted_evidence(root, gates, lookup, observer);

    for (index, gate) in gates.iter().enumerate() {
        let position = index + 1;
        observer.gate_started(position, gates.len(), gate, gate.reuse_hosted_evidence);
        let result = if gate.reuse_hosted_evidence {
            gate_result_from_hosted_evidence(root, gate, hosted.as_ref())
        } else {
            run_release_gate(root, gate)
        };
        observer.gate_finished(position, gates.len(), &result);
        let passed = result.passed;
        results.push(result);
        if fail_fast && !passed {
            stopped_early = results.len() < gates.len();
            break;
        }
    }

    GateExecutionReport {
        results,
        stopped_early,
        total_duration_ms: started.elapsed().as_millis(),
        environment_path,
    }
}

fn resolve_run_hosted_evidence(
    root: &Path,
    gates: &[ResolvedGate],
    lookup: &dyn HostedEvidenceLookup,
    observer: &mut dyn GateObserver,
) -> Option<Result<HostedRun, HostedEvidenceError>> {
    let spec = gates
        .iter()
        .find(|gate| gate.reuse_hosted_evidence)?
        .hosted_evidence
        .as_ref()?;
    observer.note("checking hosted GitHub Actions evidence");
    Some(collect_hosted_evidence(root, spec, lookup))
}

fn gate_result_from_hosted_evidence(
    root: &Path,
    gate: &ResolvedGate,
    hosted: Option<&Result<HostedRun, HostedEvidenceError>>,
) -> GateResult {
    let started_at = gate_reports::capture_started_at();
    let started = Instant::now();
    let mut result = match hosted {
        Some(Ok(run)) => GateResult {
            name: gate.name.clone(),
            description: gate.description.clone(),
            command: gate.command.clone(),
            passed: true,
            exit_code: None,
            stdout: format!(
                "reused hosted GitHub Actions run {} for commit {}",
                run.url, run.head_sha
            ),
            stderr: String::new(),
            launch_error: None,
            duration_ms: started.elapsed().as_millis(),
            log_path: None,
            reused: true,
            hosted_run_url: Some(run.url.clone()),
            hosted_head_sha: Some(run.head_sha.clone()),
            hosted_repository: Some(run.repository.clone()),
        },
        Some(Err(error)) => GateResult {
            name: gate.name.clone(),
            description: gate.description.clone(),
            command: gate.command.clone(),
            passed: false,
            exit_code: None,
            stdout: String::new(),
            stderr: error.diagnostic(&gate.name),
            launch_error: None,
            duration_ms: started.elapsed().as_millis(),
            log_path: None,
            reused: false,
            hosted_run_url: hosted_error_url(error),
            hosted_head_sha: None,
            hosted_repository: None,
        },
        None => GateResult {
            name: gate.name.clone(),
            description: gate.description.clone(),
            command: gate.command.clone(),
            passed: false,
            exit_code: None,
            stdout: String::new(),
            stderr: format!(
                "hosted evidence for gate `{}` could not be verified through authenticated `gh`: [release.hosted-evidence].workflow is not configured",
                gate.name
            ),
            launch_error: None,
            duration_ms: started.elapsed().as_millis(),
            log_path: None,
            reused: false,
            hosted_run_url: None,
            hosted_head_sha: None,
            hosted_repository: None,
        },
    };
    result.log_path = gate_reports::persist_gate_result_log(root, &result, &started_at);
    result
}

fn hosted_error_url(error: &HostedEvidenceError) -> Option<String> {
    match error {
        HostedEvidenceError::Pending { url, .. }
        | HostedEvidenceError::Failed { url, .. }
        | HostedEvidenceError::WrongRepository { url, .. }
        | HostedEvidenceError::WrongSha { url, .. } => Some(url.clone()),
        HostedEvidenceError::Missing(_)
        | HostedEvidenceError::Ambiguous { .. }
        | HostedEvidenceError::LookupFailed { .. } => None,
    }
}

pub fn collect_release_gate_run(
    repo_root: PathBuf,
    gates: Vec<ResolvedGate>,
    report: GateExecutionReport,
) -> ReleaseGateRun {
    let blockers = gate_blockers(&report.results);
    ReleaseGateRun {
        repo_root,
        configured_gate_count: gates.len(),
        gates,
        executed_gate_count: report.results.len(),
        stopped_early: report.stopped_early,
        total_duration_ms: report.total_duration_ms,
        gate_results: report.results,
        environment_path: report.environment_path,
        blockers: blockers.clone(),
        passed: blockers.is_empty(),
    }
}

pub fn run_release_gate(root: &Path, gate: &ResolvedGate) -> GateResult {
    let shell = gate_reports::resolved_shell();
    let started_at = gate_reports::capture_started_at();
    let started = Instant::now();
    let mut result = match ProcessCommand::new(&shell)
        .arg("-lc")
        .arg(&gate.command)
        .current_dir(root)
        .output()
    {
        Ok(output) => GateResult {
            name: gate.name.clone(),
            description: gate.description.clone(),
            command: gate.command.clone(),
            passed: output.status.success(),
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            launch_error: None,
            duration_ms: started.elapsed().as_millis(),
            log_path: None,
            reused: false,
            hosted_run_url: None,
            hosted_head_sha: None,
            hosted_repository: None,
        },
        Err(error) => GateResult {
            name: gate.name.clone(),
            description: gate.description.clone(),
            command: gate.command.clone(),
            passed: false,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            launch_error: Some(error.to_string()),
            duration_ms: started.elapsed().as_millis(),
            log_path: None,
            reused: false,
            hosted_run_url: None,
            hosted_head_sha: None,
            hosted_repository: None,
        },
    };
    result.log_path = gate_reports::persist_gate_result_log(root, &result, &started_at);
    result
}

pub fn load_release_context(root: &Path) -> Result<ReleaseContext, ReleaseError> {
    let config = load_release_config(root)?;
    let current_version = read_current_version(&config.version_source)?;
    let raw_changelog = std::fs::read_to_string(&config.changelog_path).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to read changelog {}: {error}",
            config.changelog_path.display()
        ))
    })?;
    let parsed_changelog = changelog::parse(&raw_changelog)
        .map_err(|error| ReleaseError::TaskInvocation(error.to_string()))?;
    let diagnostics = changelog::validate(&parsed_changelog, &raw_changelog);
    let changelog_diagnostics = diagnostics
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let unreleased_counts = unreleased_counts(&parsed_changelog);
    let unreleased_empty = unreleased_counts.values().copied().sum::<usize>() == 0;
    let initial_tag_current_version =
        config.initial_tag_current_version && parsed_changelog.latest_version().is_none();
    let suggested_bump = if initial_tag_current_version {
        BumpKind::None
    } else {
        suggested_bump(&parsed_changelog, &current_version, config.pre_1_0)
    };
    let next_version = if initial_tag_current_version {
        Some(current_version.clone())
    } else {
        apply_bump(&current_version, suggested_bump)
    };
    let tag = next_version
        .as_ref()
        .map(|version| format_release_tag(&config.tag_format, version));

    let mut blockers = Vec::new();
    if !changelog_diagnostics.is_empty() {
        blockers.push(format!(
            "changelog validation failed with {} issue(s)",
            changelog_diagnostics.len()
        ));
    }
    if let Some(version) = parsed_changelog
        .latest_version()
        .and_then(|release| release.version.clone())
    {
        if version != current_version {
            blockers.push(format!(
                "version file reports {current_version} but latest changelog release is {version}"
            ));
        }
    }
    if unreleased_empty {
        blockers.push("unreleased changelog section has no entries".to_owned());
    }
    if initial_tag_current_version {
        let selected_tag = format_release_tag(&config.tag_format, &current_version);
        if git_tag_exists(root, &selected_tag)? {
            blockers.push(format!(
                "initial release tag already exists locally: {selected_tag}"
            ));
        }
    }

    Ok(ReleaseContext {
        repo_root: root.to_path_buf(),
        config,
        current_version,
        parsed_changelog,
        changelog_diagnostics,
        unreleased_counts,
        unreleased_empty,
        suggested_bump,
        next_version,
        tag,
        blockers,
    })
}

/// Blockers that describe an absent next-version proposal rather than a failed
/// release input.
///
/// An empty `[Unreleased]` section means there is nothing to release, not that
/// the repository is unhealthy. `release status --check-gates` reports the
/// configured gate verdict independently of this optional proposal, so these
/// blockers stay visible in `blockers` but do not fail the gate check. The
/// version-requiring operations (`simulate`, `prepare`, `execute`) still
/// reject a missing version through their own readiness rules.
pub fn is_optional_next_version_blocker(blocker: &str) -> bool {
    blocker.contains("unreleased changelog section has no entries")
        || blocker.contains("no next version could be derived")
}

impl ReleaseStatus {
    /// Whether `release status --check-gates` reports success.
    ///
    /// True when gates were checked, every executed gate passed, and no
    /// blocker other than an absent next-version proposal remains. A failed
    /// gate or any other release-input blocker (invalid changelog, version
    /// mismatch, existing tag) still fails the check, as does skipping the
    /// gate run entirely: an unchecked status never reports a passed check.
    pub fn gate_check_passed(&self) -> bool {
        self.gates_passed
            && self
                .blockers
                .iter()
                .all(|blocker| is_optional_next_version_blocker(blocker))
    }
}

/// Build the `release status` report from a resolved pipeline plan.
///
/// Resolution is a pure read (contract 054 §2 R2): the plan already carries the
/// next version per identity, the files to change, the gates by stage, the
/// artifacts, the targets and the approval checkpoints. Gate execution is the
/// only thing `status` can add, and only when asked with `--check-gates`.
pub fn collect_release_status(
    plan: &ReleasePipelinePlan,
    check_gates: bool,
    gate_report: GateExecutionReport,
) -> ReleaseStatus {
    let mut blockers = plan.blockers.clone();
    if check_gates {
        blockers.extend(gate_blockers(&gate_report.results));
    }
    // No failures observed is only a pass when gates actually ran: an
    // unchecked report carries no results, which must not read as passed.
    let gates_passed = check_gates && gate_blockers(&gate_report.results).is_empty();
    let primary = plan.primary_identity();

    ReleaseStatus {
        repo_root: plan.repo_root.clone(),
        plan: plan.clone(),
        run: release_run_summary(plan),
        current_version: primary.and_then(|identity| identity.current_version.clone()),
        version_source: primary.and_then(|identity| identity.version_source.clone()),
        changelog_path: plan.changelog_path.clone(),
        changelog_valid: plan.changelog_valid,
        changelog_diagnostics: plan.changelog_diagnostics.clone(),
        unreleased_counts: plan.unreleased_counts.clone(),
        unreleased_empty: plan.unreleased_empty,
        suggested_bump: primary
            .and_then(|identity| identity.bump.clone())
            .unwrap_or_else(|| BumpKind::None.to_string()),
        next_version: primary.and_then(|identity| identity.next_version.clone()),
        tag: primary.and_then(|identity| identity.tag.clone()),
        gates_checked: check_gates,
        gates_passed,
        configured_gate_count: plan.gates.len(),
        gate_results: gate_report.results,
        environment_path: gate_report.environment_path,
        blockers: blockers.clone(),
        ready: blockers.is_empty(),
    }
}

/// The active or last release Run, read from Effigy-owned state (a pure read).
///
/// A release whose state root is absent, unreadable or belongs to another
/// Effigy schema reports no Run rather than failing `release status`.
fn release_run_summary(plan: &ReleasePipelinePlan) -> ReleaseRunSummary {
    let Ok(Some(observation)) = run::read_release_observation_for_root(&plan.repo_root) else {
        return ReleaseRunSummary::none();
    };
    let record = observation.record;
    ReleaseRunSummary {
        id: Some(record.release_id),
        stage: Some(if record.stage.is_empty() {
            "validate".to_owned()
        } else {
            record.stage
        }),
        waiting_on: record.waiting_on,
        status: Some(record.status.label().to_owned()),
        settlement: match observation.settlement {
            run::ReleaseSettlement::Interrupted => Some("pending".to_owned()),
            run::ReleaseSettlement::Incomplete => Some("incomplete".to_owned()),
            run::ReleaseSettlement::Live | run::ReleaseSettlement::Settled => None,
        },
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "../../../tests/support/scoped_env.rs"]
mod env_test_support;
