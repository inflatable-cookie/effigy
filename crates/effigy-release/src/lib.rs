mod git;
mod render_json;
mod review;
mod text;
mod version;

pub use effigy_changelog::BumpKind;
pub use git::{
    git_add_release_files, git_commit_release, git_create_tag, git_current_branch, git_head_sha,
    git_modified_files, git_push_release, git_remote_url, git_tag_exists,
};
pub use review::{
    append_indexed_review_hint, build_execute_stale_review_items,
    build_execute_working_tree_review_items, parse_blocked_preflight_action,
    parse_execute_review_action, parse_indexed_review_inspection_request,
    parse_prepare_review_action, parse_resume_menu_action, render_execute_final_review_lines,
    render_execute_review_item_detail_lines, render_execute_review_menu_lines,
    render_execute_stale_review_lines, render_execute_state_review_lines,
    render_execute_working_tree_review_lines, render_prepare_final_review_lines,
    render_prepare_gate_review_lines, render_prepare_mutation_detail_lines,
    render_prepare_mutation_review_lines, render_prepare_review_menu_lines,
    render_prepare_version_review_lines, render_release_gate_run_lines,
    render_release_reprepare_handoff_lines, render_release_resume_drift_lines,
    render_release_resume_menu_lines, render_release_state_discard_confirmation_lines,
    BlockedPreflightAction, ExecuteMenuAction, ExecuteReviewItem, ExecuteReviewState,
    PrepareMenuAction, PrepareReviewState, ResumeMenuAction,
};
pub use render_json::{
    render_release_execute_plan_json, render_release_executed_json,
    render_release_gate_run_json, render_release_prepare_plan_json,
    render_release_prepared_json, render_release_resume_json,
    render_release_simulation_json, render_release_status_json,
    render_release_verify_install_json,
};
pub use text::{
    format_counts, remediation_hints_for_blockers, render_release_execute_plan_text,
    render_release_executed_text, render_release_gate_run_text, render_release_prepare_plan_text,
    render_release_prepared_text, render_release_resume_text, render_release_simulation_text,
    render_release_state_discarded_text, render_release_status_text,
    render_release_verify_install_text, review_label, ReleaseBlockedStage,
};
pub use version::{
    build_changelog_mutation_detail_lines, build_diff_preview,
    build_version_mutation_detail_lines, detect_pyproject_version_path,
    detect_version_file_kind, json_value_at_path, read_current_version,
    render_changelog_preview_line, render_updated_version_contents,
    render_version_preview_line, replace_json_string_at_path_preserving_layout,
    resolve_version_field_path, toml_value_at_path,
};

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::time::Instant;

use chrono::{DateTime, Utc};
use effigy_changelog::{self as changelog, CategoryKind};
use effigy_manifest::config_sections::{
    ManifestReleaseConfig, ManifestReleaseGateConfig, ManifestReleaseGateDetails,
};
use effigy_manifest::{load_task_manifest, ManifestError};
use serde::Deserialize;
use serde_json::json;

const TASK_MANIFEST_FILE: &str = "effigy.toml";

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
    pub command: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedSyncFile {
    pub path: PathBuf,
    pub kind: SyncFileKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncFileKind {
    CargoLock,
}

#[derive(Debug, Clone)]
pub struct ReleaseConfig {
    pub version_source: ResolvedVersionSource,
    pub changelog_path: PathBuf,
    pub pre_1_0: bool,
    pub sync_files: Vec<ResolvedSyncFile>,
    pub gates: Vec<ResolvedGate>,
    pub tag_format: String,
}

#[derive(Debug, Clone)]
pub struct GateResult {
    pub name: String,
    pub description: Option<String>,
    pub command: String,
    pub passed: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub launch_error: Option<String>,
    pub duration_ms: u128,
}

#[derive(Debug, Clone)]
pub struct GateExecutionReport {
    pub results: Vec<GateResult>,
    pub stopped_early: bool,
    pub total_duration_ms: u128,
}

#[derive(Debug, Clone)]
pub struct ReleaseStatus {
    pub repo_root: PathBuf,
    pub current_version: semver::Version,
    pub version_source: ResolvedVersionSource,
    pub changelog_path: PathBuf,
    pub changelog_valid: bool,
    pub changelog_diagnostics: Vec<String>,
    pub unreleased_counts: BTreeMap<String, usize>,
    pub unreleased_empty: bool,
    pub suggested_bump: String,
    pub next_version: Option<semver::Version>,
    pub tag: Option<String>,
    pub gates_checked: bool,
    pub configured_gate_count: usize,
    pub gate_results: Vec<GateResult>,
    pub blockers: Vec<String>,
    pub ready: bool,
}

#[derive(Debug, Clone)]
pub struct ReleaseGateRun {
    pub repo_root: PathBuf,
    pub configured_gate_count: usize,
    pub executed_gate_count: usize,
    pub stopped_early: bool,
    pub total_duration_ms: u128,
    pub gate_results: Vec<GateResult>,
    pub blockers: Vec<String>,
    pub passed: bool,
}

#[derive(Debug, Clone)]
pub struct VerificationStepResult {
    pub name: String,
    pub command: String,
    pub passed: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub launch_error: Option<String>,
    pub duration_ms: u128,
}

#[derive(Debug, Clone)]
pub struct ReleaseVerifyInstall {
    pub repo_root: PathBuf,
    pub tag: String,
    pub repo_url: String,
    pub installed_bin: Option<PathBuf>,
    pub configured_check_count: usize,
    pub executed_check_count: usize,
    pub stopped_early: bool,
    pub results: Vec<VerificationStepResult>,
    pub blockers: Vec<String>,
    pub verified: bool,
}

#[derive(Debug, Clone)]
pub struct FileMutationPlan {
    pub path: PathBuf,
    pub kind: &'static str,
    pub summary: String,
    pub before_preview: String,
    pub after_preview: String,
    pub detail_lines: Vec<String>,
    pub diff_preview: Vec<String>,
    pub apply: FileMutationApply,
}

#[derive(Debug, Clone)]
pub enum FileMutationApply {
    Write { after_contents: String },
    SyncCargoLock,
}

#[derive(Debug, Clone)]
pub struct ReleasePreparePlan {
    pub repo_root: PathBuf,
    pub current_version: semver::Version,
    pub version_source: ResolvedVersionSource,
    pub suggested_version: Option<semver::Version>,
    pub planned_version: Option<semver::Version>,
    pub suggested_tag: Option<String>,
    pub tag: Option<String>,
    pub version_override_used: bool,
    pub release_date: String,
    pub gates_checked: bool,
    pub configured_gate_count: usize,
    pub gate_results: Vec<GateResult>,
    pub blockers: Vec<String>,
    pub mutations: Vec<FileMutationPlan>,
    pub ready: bool,
}

#[derive(Debug, Clone)]
pub struct ReleasePrepared {
    pub repo_root: PathBuf,
    pub previous_version: semver::Version,
    pub suggested_version: Option<semver::Version>,
    pub prepared_version: Option<semver::Version>,
    pub suggested_tag: Option<String>,
    pub tag: Option<String>,
    pub version_override_used: bool,
    pub release_date: String,
    pub state_file: PathBuf,
    pub gates_checked: bool,
    pub configured_gate_count: usize,
    pub gate_results: Vec<GateResult>,
    pub files_modified: Vec<PathBuf>,
    pub blockers: Vec<String>,
    pub prepared: bool,
    pub state_file_written: bool,
}

#[derive(Debug, Clone)]
pub struct ReleaseSimulation {
    pub repo_root: PathBuf,
    pub current_version: semver::Version,
    pub version_source: ResolvedVersionSource,
    pub suggested_version: Option<semver::Version>,
    pub planned_version: Option<semver::Version>,
    pub suggested_tag: Option<String>,
    pub tag: Option<String>,
    pub version_override_used: bool,
    pub release_date: String,
    pub state_file: PathBuf,
    pub state_file_exists: bool,
    pub state_file_written: bool,
    pub commit_message: Option<String>,
    pub configured_gate_count: usize,
    pub executed_gate_count: usize,
    pub stopped_early: bool,
    pub total_duration_ms: u128,
    pub gate_results: Vec<GateResult>,
    pub mutations: Vec<FileMutationPlan>,
    pub blockers: Vec<String>,
    pub ready: bool,
}

#[derive(Debug, Clone)]
pub struct ReleaseExecutePlan {
    pub repo_root: PathBuf,
    pub state_file: PathBuf,
    pub previous_version: Option<semver::Version>,
    pub suggested_version: Option<semver::Version>,
    pub prepared_version: Option<semver::Version>,
    pub suggested_tag: Option<String>,
    pub tag: Option<String>,
    pub version_override_used: bool,
    pub release_date: Option<String>,
    pub prepared_at: Option<String>,
    pub state_loaded: bool,
    pub stale: bool,
    pub stale_threshold_seconds: i64,
    pub stale_override_required: bool,
    pub stale_override_used: bool,
    pub gates_checked: bool,
    pub gates_passed: bool,
    pub prepared_branch: Option<String>,
    pub prepared_head: Option<String>,
    pub branch: Option<String>,
    pub current_head: Option<String>,
    pub remote: Option<String>,
    pub expected_files: Vec<String>,
    pub modified_files: Vec<String>,
    pub missing_expected_files: Vec<String>,
    pub unexpected_files: Vec<String>,
    pub source_fingerprint_available: bool,
    pub fingerprint_drift: Vec<String>,
    pub warnings: Vec<String>,
    pub blockers: Vec<String>,
    pub ready: bool,
}

#[derive(Debug, Clone)]
pub struct ReleaseContext {
    pub repo_root: PathBuf,
    pub config: ReleaseConfig,
    pub current_version: semver::Version,
    pub parsed_changelog: changelog::Changelog,
    pub changelog_diagnostics: Vec<String>,
    pub unreleased_counts: BTreeMap<String, usize>,
    pub unreleased_empty: bool,
    pub suggested_bump: BumpKind,
    pub next_version: Option<semver::Version>,
    pub tag: Option<String>,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ReleaseExecuted {
    pub repo_root: PathBuf,
    pub state_file: PathBuf,
    pub previous_version: Option<semver::Version>,
    pub suggested_version: Option<semver::Version>,
    pub prepared_version: Option<semver::Version>,
    pub suggested_tag: Option<String>,
    pub tag: Option<String>,
    pub version_override_used: bool,
    pub release_date: Option<String>,
    pub prepared_at: Option<String>,
    pub prepared_branch: Option<String>,
    pub prepared_head: Option<String>,
    pub branch: Option<String>,
    pub current_head: Option<String>,
    pub remote: Option<String>,
    pub commit_message: Option<String>,
    pub commit_sha: Option<String>,
    pub stale: bool,
    pub stale_override_used: bool,
    pub fingerprint_drift: Vec<String>,
    pub warnings: Vec<String>,
    pub blockers: Vec<String>,
    pub files_committed: Vec<String>,
    pub state_file_removed: bool,
    pub committed: bool,
    pub tag_created: bool,
    pub pushed: bool,
    pub executed: bool,
    pub post_release_instructions: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ReleasePreparedState {
    pub previous_version: semver::Version,
    pub suggested_version: Option<semver::Version>,
    pub prepared_version: semver::Version,
    pub suggested_tag: Option<String>,
    pub tag: Option<String>,
    pub version_override_used: bool,
    pub release_date: Option<String>,
    pub prepared_at: DateTime<Utc>,
    pub prepared_at_raw: String,
    pub gates_checked: bool,
    pub gates_passed: bool,
    pub files_modified: Vec<PathBuf>,
    pub source_fingerprints: Option<ReleasePreparedSourceFingerprints>,
}

#[derive(Debug, Clone)]
pub struct ReleasePreparedSourceFingerprints {
    pub prepared_branch: Option<String>,
    pub prepared_head: Option<String>,
    pub files: Vec<ReleasePreparedFileFingerprint>,
}

#[derive(Debug, Clone)]
pub struct ReleasePreparedFileFingerprint {
    pub path: PathBuf,
    pub digest: String,
}

#[derive(Debug, Deserialize)]
struct RawReleasePreparedState {
    schema: String,
    previous_version: String,
    suggested_version: Option<String>,
    version: Option<String>,
    suggested_tag: Option<String>,
    tag: Option<String>,
    version_override_used: Option<bool>,
    release_date: Option<String>,
    prepared_at: String,
    gates_checked: Option<bool>,
    gates_passed: Option<bool>,
    files_modified: Vec<String>,
    source_fingerprints: Option<RawReleasePreparedSourceFingerprints>,
}

#[derive(Debug, Deserialize)]
struct RawReleasePreparedSourceFingerprints {
    prepared_branch: Option<String>,
    prepared_head: Option<String>,
    files: Vec<RawReleasePreparedFileFingerprint>,
}

#[derive(Debug, Deserialize)]
struct RawReleasePreparedFileFingerprint {
    path: String,
    digest: String,
}

impl GateExecutionReport {
    pub fn empty() -> Self {
        Self {
            results: Vec::new(),
            stopped_early: false,
            total_duration_ms: 0,
        }
    }
}

#[derive(Debug)]
pub enum ReleaseError {
    Manifest(ManifestError),
    TaskInvocation(String),
}

impl std::fmt::Display for ReleaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Manifest(error) => write!(f, "{error}"),
            Self::TaskInvocation(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ReleaseError {}

impl From<ManifestError> for ReleaseError {
    fn from(value: ManifestError) -> Self {
        Self::Manifest(value)
    }
}

pub fn load_release_config(root: &Path) -> Result<ReleaseConfig, ReleaseError> {
    let manifest_path = root.join(TASK_MANIFEST_FILE);
    let manifest = if manifest_path.exists() {
        Some(load_task_manifest(&manifest_path)?)
    } else {
        None
    };
    let manifest_release = manifest.as_ref().and_then(|parsed| parsed.release.as_ref());
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
        sync_files,
        gates,
        tag_format,
    })
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
        let field_path = resolve_version_field_path(
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
        let field_path = resolve_version_field_path(kind, None)?;
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
    let mut gates = Vec::with_capacity(config.gates.len());
    for (name, gate) in &config.gates {
        let (command, description) = match gate {
            ManifestReleaseGateConfig::Command(command) => (command.trim(), None),
            ManifestReleaseGateConfig::Detailed(ManifestReleaseGateDetails {
                command,
                description,
            }) => (command.trim(), description.clone()),
        };
        if command.is_empty() {
            return Err(ReleaseError::TaskInvocation(format!(
                "release gate `{name}` must not have an empty command"
            )));
        }
        gates.push(ResolvedGate {
            name: name.clone(),
            command: command.to_owned(),
            description,
        });
    }
    Ok(gates)
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
        let path = root.join(trimmed);
        if !seen.insert(path.clone()) {
            continue;
        }
        match path.file_name().and_then(|name| name.to_str()) {
            Some("Cargo.lock") if matches!(version_source.kind, VersionFileKind::CargoToml) => {
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
            Some(other) => {
                return Err(ReleaseError::TaskInvocation(format!(
                    "unsupported release.sync-files entry `{other}`; currently only `Cargo.lock` is supported"
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
    run_release_gates_with_progress(root, gates, fail_fast, |_| {})
}

pub fn run_release_gates_with_progress<F>(
    root: &Path,
    gates: &[ResolvedGate],
    fail_fast: bool,
    mut progress: F,
) -> GateExecutionReport
where
    F: FnMut(&str),
{
    let started = Instant::now();
    let mut results = Vec::with_capacity(gates.len());
    let mut stopped_early = false;

    for gate in gates {
        progress(&format!("running gate `{}`", gate.name));
        let result = run_release_gate(root, gate);
        progress(&format!(
            "gate `{}` {} ({})",
            result.name,
            if result.passed { "passed" } else { "failed" },
            format_duration_ms(result.duration_ms),
        ));
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
    }
}

pub fn collect_release_gate_run(
    repo_root: PathBuf,
    configured_gate_count: usize,
    report: GateExecutionReport,
) -> ReleaseGateRun {
    let blockers = gate_blockers(&report.results);
    ReleaseGateRun {
        repo_root,
        configured_gate_count,
        executed_gate_count: report.results.len(),
        stopped_early: report.stopped_early,
        total_duration_ms: report.total_duration_ms,
        gate_results: report.results,
        blockers: blockers.clone(),
        passed: blockers.is_empty(),
    }
}

pub fn run_release_gate(root: &Path, gate: &ResolvedGate) -> GateResult {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_owned());
    let started = Instant::now();
    match ProcessCommand::new(&shell)
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
        },
    }
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
    let suggested_bump = suggested_bump(&parsed_changelog, &current_version, config.pre_1_0);
    let next_version = apply_bump(&current_version, suggested_bump);
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

pub fn collect_release_status(
    context: &ReleaseContext,
    check_gates: bool,
    gate_report: GateExecutionReport,
) -> ReleaseStatus {
    let mut blockers = context.blockers.clone();
    if check_gates {
        blockers.extend(gate_blockers(&gate_report.results));
    }

    ReleaseStatus {
        repo_root: context.repo_root.clone(),
        current_version: context.current_version.clone(),
        version_source: context.config.version_source.clone(),
        changelog_path: context.config.changelog_path.clone(),
        changelog_valid: context.changelog_diagnostics.is_empty(),
        changelog_diagnostics: context.changelog_diagnostics.clone(),
        unreleased_counts: context.unreleased_counts.clone(),
        unreleased_empty: context.unreleased_empty,
        suggested_bump: context.suggested_bump.to_string(),
        next_version: context.next_version.clone(),
        tag: context.tag.clone(),
        gates_checked: check_gates,
        configured_gate_count: context.config.gates.len(),
        gate_results: gate_report.results,
        blockers: blockers.clone(),
        ready: blockers.is_empty(),
    }
}

pub fn build_release_prepare_plan(
    context: &ReleaseContext,
    check_gates: bool,
    gate_report: GateExecutionReport,
    version_override: Option<semver::Version>,
) -> Result<ReleasePreparePlan, ReleaseError> {
    let release_date = Utc::now().date_naive().to_string();
    let mut blockers = context.blockers.clone();
    let mut mutations = Vec::new();
    let suggested_version = context.next_version.clone();
    let suggested_tag = suggested_version
        .as_ref()
        .map(|version| format_release_tag(&context.config.tag_format, version));
    let version_override_used = version_override.is_some();

    let Some(next_version) = version_override.or_else(|| suggested_version.clone()) else {
        blockers.push("no next version could be derived from changelog content".to_owned());
        blockers.extend(gate_blockers_if_checked(check_gates, &gate_report.results));
        return Ok(ReleasePreparePlan {
            repo_root: context.repo_root.clone(),
            current_version: context.current_version.clone(),
            version_source: context.config.version_source.clone(),
            suggested_version,
            planned_version: None,
            suggested_tag,
            tag: None,
            version_override_used,
            release_date,
            gates_checked: check_gates,
            configured_gate_count: context.config.gates.len(),
            gate_results: gate_report.results,
            blockers: blockers.clone(),
            mutations,
            ready: false,
        });
    };
    let selected_tag = format_release_tag(&context.config.tag_format, &next_version);

    if context
        .parsed_changelog
        .find_version(&next_version.to_string())
        .is_some()
    {
        blockers.push(format!(
            "changelog already contains release version {}",
            next_version
        ));
    }

    if blockers.is_empty() {
        let version_before =
            std::fs::read_to_string(&context.config.version_source.path).map_err(|error| {
                ReleaseError::TaskInvocation(format!(
                    "failed to read release version file {}: {error}",
                    context.config.version_source.path.display()
                ))
            })?;
        let changelog_before =
            std::fs::read_to_string(&context.config.changelog_path).map_err(|error| {
                ReleaseError::TaskInvocation(format!(
                    "failed to read changelog {}: {error}",
                    context.config.changelog_path.display()
                ))
            })?;
        let version_after =
            render_updated_version_contents(&context.config.version_source, &next_version)?;
        let changelog_after = render_prepared_changelog_contents(
            &context.parsed_changelog,
            &next_version,
            &release_date,
        )?;

        mutations.push(FileMutationPlan {
            path: context.config.version_source.path.clone(),
            kind: "version-file",
            summary: format!(
                "update version from {} to {}",
                context.current_version, next_version
            ),
            before_preview: render_version_preview_line(
                &context.config.version_source,
                &version_before,
                &context.current_version.to_string(),
            ),
            after_preview: render_version_preview_line(
                &context.config.version_source,
                &version_after,
                &next_version.to_string(),
            ),
            detail_lines: build_version_mutation_detail_lines(
                &context.config.version_source,
                &next_version,
            ),
            diff_preview: build_diff_preview(&version_before, &version_after),
            apply: FileMutationApply::Write {
                after_contents: version_after.clone(),
            },
        });
        mutations.push(FileMutationPlan {
            path: context.config.changelog_path.clone(),
            kind: "changelog",
            summary: format!(
                "promote [Unreleased] into [{}] - {} and reset [Unreleased]",
                next_version, release_date
            ),
            before_preview: format!(
                "[Unreleased] currently contains {}",
                format_counts(&context.unreleased_counts)
            ),
            after_preview: render_changelog_preview_line(
                &changelog_after,
                &next_version,
                &release_date,
            ),
            detail_lines: build_changelog_mutation_detail_lines(
                &context.unreleased_counts,
                &next_version,
                &release_date,
            ),
            diff_preview: build_diff_preview(&changelog_before, &changelog_after),
            apply: FileMutationApply::Write {
                after_contents: changelog_after.clone(),
            },
        });
        mutations.extend(build_sync_mutations(&context.config.sync_files));
    }

    blockers.extend(gate_blockers_if_checked(check_gates, &gate_report.results));

    Ok(ReleasePreparePlan {
        repo_root: context.repo_root.clone(),
        current_version: context.current_version.clone(),
        version_source: context.config.version_source.clone(),
        suggested_version,
        planned_version: Some(next_version),
        suggested_tag,
        tag: Some(selected_tag),
        version_override_used,
        release_date,
        gates_checked: check_gates,
        configured_gate_count: context.config.gates.len(),
        gate_results: gate_report.results,
        blockers: blockers.clone(),
        mutations,
        ready: blockers.is_empty(),
    })
}

pub fn collect_release_simulation(
    root: &Path,
    state_file_name: &str,
    prepare_plan: ReleasePreparePlan,
    gate_report: &GateExecutionReport,
) -> ReleaseSimulation {
    let state_file = root.join(state_file_name);
    let state_file_exists = state_file.exists();
    let mut blockers = prepare_plan.blockers.clone();
    if state_file_exists {
        blockers.push(format!(
            "release state file already exists and would block prepare: {}",
            state_file.display()
        ));
    }

    ReleaseSimulation {
        repo_root: prepare_plan.repo_root,
        current_version: prepare_plan.current_version,
        version_source: prepare_plan.version_source,
        suggested_version: prepare_plan.suggested_version.clone(),
        planned_version: prepare_plan.planned_version.clone(),
        suggested_tag: prepare_plan.suggested_tag.clone(),
        tag: prepare_plan.tag.clone(),
        version_override_used: prepare_plan.version_override_used,
        release_date: prepare_plan.release_date,
        state_file,
        state_file_exists,
        state_file_written: false,
        commit_message: prepare_plan
            .planned_version
            .as_ref()
            .map(|version| format!("release: v{version}")),
        configured_gate_count: prepare_plan.configured_gate_count,
        executed_gate_count: gate_report.results.len(),
        stopped_early: gate_report.stopped_early,
        total_duration_ms: gate_report.total_duration_ms,
        gate_results: gate_report.results.clone(),
        mutations: prepare_plan.mutations,
        blockers: blockers.clone(),
        ready: blockers.is_empty(),
    }
}

pub fn collect_release_execute_plan(
    repo_root: PathBuf,
    state_file_name: &str,
    stale_threshold_seconds: i64,
    allow_stale: bool,
) -> Result<ReleaseExecutePlan, ReleaseError> {
    let state_file = repo_root.join(state_file_name);
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();
    let mut previous_version = None;
    let mut suggested_version = None;
    let mut prepared_version = None;
    let mut suggested_tag = None;
    let mut tag = None;
    let mut version_override_used = false;
    let mut release_date = None;
    let mut prepared_at = None;
    let mut state_loaded = false;
    let mut stale = false;
    let mut stale_override_required = false;
    let mut stale_override_used = false;
    let mut gates_checked = false;
    let mut gates_passed = false;
    let mut prepared_branch = None;
    let mut prepared_head = None;
    let mut branch = None;
    let mut current_head = None;
    let mut remote = None;
    let mut expected_files = Vec::new();
    let mut modified_files = Vec::new();
    let mut missing_expected_files = Vec::new();
    let mut unexpected_files = Vec::new();
    let mut source_fingerprint_available = false;
    let mut fingerprint_drift = Vec::new();

    if !state_file.exists() {
        blockers.push(format!(
            "release state file does not exist: {}",
            state_file.display()
        ));
    } else {
        match load_release_prepared_state(&state_file) {
            Ok(state) => {
                state_loaded = true;
                previous_version = Some(state.previous_version.clone());
                suggested_version = state.suggested_version.clone();
                prepared_version = Some(state.prepared_version.clone());
                suggested_tag = state.suggested_tag.clone();
                tag = state.tag.clone();
                version_override_used = state.version_override_used;
                release_date = state.release_date.clone();
                prepared_at = Some(state.prepared_at_raw.clone());
                gates_checked = state.gates_checked;
                gates_passed = state.gates_passed;
                prepared_branch = state
                    .source_fingerprints
                    .as_ref()
                    .and_then(|fingerprints| fingerprints.prepared_branch.clone());
                prepared_head = state
                    .source_fingerprints
                    .as_ref()
                    .and_then(|fingerprints| fingerprints.prepared_head.clone());
                source_fingerprint_available = state.source_fingerprints.is_some();

                let age = Utc::now().signed_duration_since(state.prepared_at);
                if age > chrono::Duration::seconds(stale_threshold_seconds) {
                    stale = true;
                    stale_override_required = !allow_stale;
                    stale_override_used = allow_stale;
                    warnings.push(format!(
                        "release state is stale: prepared {} seconds ago (threshold: {} seconds)",
                        age.num_seconds(),
                        stale_threshold_seconds
                    ));
                    if !allow_stale {
                        blockers.push(
                            "release state is stale; rerun `effigy release prepare` or pass `--allow-stale` to acknowledge and continue"
                                .to_owned(),
                        );
                    }
                }
                if !state.gates_passed {
                    blockers
                        .push("prepared release state reports failed or skipped gates".to_owned());
                }
                match git_current_branch(&repo_root) {
                    Ok(current_branch) => branch = Some(current_branch),
                    Err(error) => blockers.push(error.to_string()),
                }
                match git_head_sha(&repo_root) {
                    Ok(head) => current_head = Some(head),
                    Err(error) => blockers.push(error.to_string()),
                }
                match git_remote_url(&repo_root, "origin") {
                    Ok(url) => remote = Some(url),
                    Err(error) => blockers.push(error.to_string()),
                }
                if let Some(prepared_tag) = state.tag.as_deref() {
                    match git_tag_exists(&repo_root, prepared_tag) {
                        Ok(true) => blockers.push(format!(
                            "release tag already exists locally: {prepared_tag}"
                        )),
                        Ok(false) => {}
                        Err(error) => blockers.push(error.to_string()),
                    }
                }

                expected_files =
                    normalized_expected_files(state_file_name, &repo_root, &state.files_modified);
                match git_modified_files(&repo_root) {
                    Ok(paths) => {
                        modified_files = paths;
                        let expected_set = expected_files.iter().cloned().collect::<BTreeSet<_>>();
                        let modified_set = modified_files.iter().cloned().collect::<BTreeSet<_>>();
                        missing_expected_files = expected_set
                            .difference(&modified_set)
                            .cloned()
                            .collect::<Vec<_>>();
                        unexpected_files = modified_set
                            .difference(&expected_set)
                            .cloned()
                            .collect::<Vec<_>>();
                        if !missing_expected_files.is_empty() {
                            blockers.push(format!(
                                "working tree is missing {} expected prepared file change(s)",
                                missing_expected_files.len()
                            ));
                        }
                        if !unexpected_files.is_empty() {
                            blockers.push(format!(
                                "working tree contains {} unexpected change(s)",
                                unexpected_files.len()
                            ));
                        }
                    }
                    Err(error) => blockers.push(error.to_string()),
                }

                if let Some(fingerprints) = &state.source_fingerprints {
                    fingerprint_drift = compare_release_state_fingerprints(
                        &repo_root,
                        fingerprints,
                        branch.as_deref(),
                        current_head.as_deref(),
                    );
                    if !fingerprint_drift.is_empty() {
                        blockers.push(format!(
                            "prepared release source drift detected in {} place(s)",
                            fingerprint_drift.len()
                        ));
                    }
                } else {
                    warnings.push(
                        "release state does not record source fingerprints; branch, HEAD, and content drift checks are limited"
                            .to_owned(),
                    );
                }
            }
            Err(error) => blockers.push(error.to_string()),
        }
    }

    Ok(ReleaseExecutePlan {
        repo_root,
        state_file,
        previous_version,
        suggested_version,
        prepared_version,
        suggested_tag,
        tag,
        version_override_used,
        release_date,
        prepared_at,
        state_loaded,
        stale,
        stale_threshold_seconds,
        stale_override_required,
        stale_override_used,
        gates_checked,
        gates_passed,
        prepared_branch,
        prepared_head,
        branch,
        current_head,
        remote,
        expected_files,
        modified_files,
        missing_expected_files,
        unexpected_files,
        source_fingerprint_available,
        fingerprint_drift,
        warnings,
        blockers: blockers.clone(),
        ready: blockers.is_empty(),
    })
}

pub fn execute_release_prepare<F>(
    repo_root: PathBuf,
    state_file_name: &str,
    check_gates: bool,
    version_override: Option<semver::Version>,
    mut progress: F,
) -> Result<ReleasePrepared, ReleaseError>
where
    F: FnMut(&str),
{
    let context = load_release_context(&repo_root)?;
    let plan = build_release_prepare_plan(
        &context,
        false,
        GateExecutionReport::empty(),
        version_override,
    )?;
    let state_file = repo_root.join(state_file_name);
    let mut blockers = plan.blockers.clone();

    if state_file.exists() {
        blockers.push(format!(
            "release state file already exists: {}",
            state_file.display()
        ));
    }
    if !check_gates && !context.config.gates.is_empty() {
        blockers.push(
            "release prepare requires `--check-gates` when `[release.gates]` is configured"
                .to_owned(),
        );
    }

    let prepared_version = plan.planned_version.clone();
    let planned_files = plan
        .mutations
        .iter()
        .map(|mutation| mutation.path.clone())
        .collect::<Vec<_>>();
    if !blockers.is_empty() {
        return Ok(ReleasePrepared {
            repo_root,
            previous_version: context.current_version,
            suggested_version: plan.suggested_version.clone(),
            prepared_version,
            suggested_tag: plan.suggested_tag.clone(),
            tag: plan.tag.clone(),
            version_override_used: plan.version_override_used,
            release_date: plan.release_date,
            state_file,
            gates_checked: false,
            configured_gate_count: context.config.gates.len(),
            gate_results: Vec::new(),
            files_modified: planned_files,
            blockers,
            prepared: false,
            state_file_written: false,
        });
    }

    let snapshots = snapshot_mutation_paths(&plan.mutations)?;
    if let Err(error) = apply_release_mutations(&repo_root, &plan.mutations) {
        let files_modified = collect_changed_mutation_paths(&plan.mutations, &snapshots)
            .unwrap_or_else(|_| planned_files.clone());
        return Ok(ReleasePrepared {
            repo_root,
            previous_version: context.current_version,
            suggested_version: plan.suggested_version.clone(),
            prepared_version,
            suggested_tag: plan.suggested_tag.clone(),
            tag: plan.tag.clone(),
            version_override_used: plan.version_override_used,
            release_date: plan.release_date,
            state_file,
            gates_checked: false,
            configured_gate_count: context.config.gates.len(),
            gate_results: Vec::new(),
            files_modified,
            blockers: vec![error.to_string()],
            prepared: false,
            state_file_written: false,
        });
    }

    let gate_report = if check_gates {
        progress("re-running release gates against prepared files");
        run_release_gates_with_progress(&repo_root, &context.config.gates, true, |message| {
            progress(message);
        })
    } else {
        GateExecutionReport::empty()
    };
    let gate_blockers = gate_blockers_if_checked(check_gates, &gate_report.results);
    let files_modified =
        collect_changed_mutation_paths(&plan.mutations, &snapshots).unwrap_or(planned_files);
    if !gate_blockers.is_empty() {
        return Ok(ReleasePrepared {
            repo_root,
            previous_version: context.current_version,
            suggested_version: plan.suggested_version.clone(),
            prepared_version,
            suggested_tag: plan.suggested_tag.clone(),
            tag: plan.tag.clone(),
            version_override_used: plan.version_override_used,
            release_date: plan.release_date,
            state_file,
            gates_checked: check_gates,
            configured_gate_count: context.config.gates.len(),
            gate_results: gate_report.results,
            files_modified,
            blockers: gate_blockers,
            prepared: false,
            state_file_written: false,
        });
    }

    let prepared_branch = git_current_branch(&repo_root).ok();
    let prepared_head = git_head_sha(&repo_root).ok();
    write_release_prepared_state(
        &state_file,
        &repo_root,
        &context.current_version,
        plan.suggested_version.as_ref(),
        prepared_version.as_ref(),
        plan.suggested_tag.as_deref(),
        plan.tag.as_deref(),
        plan.version_override_used,
        &plan.release_date,
        check_gates,
        &files_modified,
        prepared_branch.as_deref(),
        prepared_head.as_deref(),
    )?;

    Ok(ReleasePrepared {
        repo_root,
        previous_version: context.current_version,
        suggested_version: plan.suggested_version,
        prepared_version,
        suggested_tag: plan.suggested_tag,
        tag: plan.tag,
        version_override_used: plan.version_override_used,
        release_date: plan.release_date,
        state_file,
        gates_checked: check_gates,
        configured_gate_count: context.config.gates.len(),
        gate_results: gate_report.results,
        files_modified,
        blockers: Vec::new(),
        prepared: true,
        state_file_written: true,
    })
}

pub fn execute_release<F>(
    repo_root: PathBuf,
    state_file_name: &str,
    stale_threshold_seconds: i64,
    allow_stale: bool,
    mut progress: F,
) -> Result<ReleaseExecuted, ReleaseError>
where
    F: FnMut(&str),
{
    let plan = collect_release_execute_plan(
        repo_root.clone(),
        state_file_name,
        stale_threshold_seconds,
        allow_stale,
    )?;
    let state = load_release_prepared_state(&plan.state_file).ok();
    let files_committed = state
        .as_ref()
        .map(|loaded| normalized_repo_files(&repo_root, &loaded.files_modified))
        .unwrap_or_default();
    let commit_message = plan
        .prepared_version
        .as_ref()
        .map(|version| format!("release: v{version}"));

    if !plan.ready {
        return Ok(ReleaseExecuted {
            repo_root: plan.repo_root,
            state_file: plan.state_file,
            previous_version: plan.previous_version,
            suggested_version: plan.suggested_version,
            prepared_version: plan.prepared_version,
            suggested_tag: plan.suggested_tag,
            tag: plan.tag,
            version_override_used: plan.version_override_used,
            release_date: plan.release_date,
            prepared_at: plan.prepared_at,
            prepared_branch: plan.prepared_branch,
            prepared_head: plan.prepared_head,
            branch: plan.branch,
            current_head: plan.current_head,
            remote: plan.remote,
            commit_message,
            commit_sha: None,
            stale: plan.stale,
            stale_override_used: plan.stale_override_used,
            fingerprint_drift: plan.fingerprint_drift,
            warnings: plan.warnings,
            blockers: plan.blockers,
            files_committed,
            state_file_removed: false,
            committed: false,
            tag_created: false,
            pushed: false,
            executed: false,
            post_release_instructions: Vec::new(),
        });
    }

    let Some(state) = state else {
        return Ok(ReleaseExecuted {
            repo_root: plan.repo_root,
            state_file: plan.state_file,
            previous_version: plan.previous_version,
            suggested_version: plan.suggested_version,
            prepared_version: plan.prepared_version,
            suggested_tag: plan.suggested_tag,
            tag: plan.tag,
            version_override_used: plan.version_override_used,
            release_date: plan.release_date,
            prepared_at: plan.prepared_at,
            prepared_branch: plan.prepared_branch,
            prepared_head: plan.prepared_head,
            branch: plan.branch,
            current_head: plan.current_head,
            remote: plan.remote,
            commit_message,
            commit_sha: None,
            stale: plan.stale,
            stale_override_used: plan.stale_override_used,
            fingerprint_drift: plan.fingerprint_drift,
            warnings: plan.warnings,
            blockers: vec!["release state became unreadable during execute".to_owned()],
            files_committed,
            state_file_removed: false,
            committed: false,
            tag_created: false,
            pushed: false,
            executed: false,
            post_release_instructions: Vec::new(),
        });
    };

    let branch = plan.branch.clone();
    let remote = plan.remote.clone();
    let tag = plan.tag.clone();
    let mut blockers = Vec::new();
    let mut commit_sha = None;
    let mut committed = false;
    let mut tag_created = false;
    let mut pushed = false;
    let mut state_file_removed = false;

    if let Err(error) = git_add_release_files(&repo_root, &state.files_modified) {
        blockers.push(error.to_string());
    } else {
        progress("creating release commit");
        match git_commit_release(
            &repo_root,
            commit_message.as_deref().unwrap_or("release: vunknown"),
        ) {
            Ok(sha) => {
                commit_sha = Some(sha);
                committed = true;
            }
            Err(error) => blockers.push(error.to_string()),
        }
    }

    if blockers.is_empty() {
        if let Some(prepared_tag) = tag.as_deref() {
            progress(&format!("creating tag `{prepared_tag}`"));
            match git_create_tag(&repo_root, prepared_tag) {
                Ok(()) => tag_created = true,
                Err(error) => blockers.push(error.to_string()),
            }
        }
    }

    if blockers.is_empty() {
        progress("pushing release commit and tag");
        match git_push_release(
            &repo_root,
            branch.as_deref().unwrap_or("HEAD"),
            "origin",
            tag.as_deref(),
        ) {
            Ok(()) => pushed = true,
            Err(error) => blockers.push(error.to_string()),
        }
    }

    if blockers.is_empty() {
        progress(&format!("removing {}", plan.state_file.display()));
        std::fs::remove_file(&plan.state_file).map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "failed to remove release state file {}: {error}",
                plan.state_file.display()
            ))
        })?;
        state_file_removed = true;
    }

    let executed = blockers.is_empty() && committed && pushed;
    let post_release_instructions = if executed {
        build_post_release_instructions(tag.as_deref())
    } else {
        Vec::new()
    };

    Ok(ReleaseExecuted {
        repo_root: plan.repo_root,
        state_file: plan.state_file,
        previous_version: plan.previous_version,
        suggested_version: plan.suggested_version,
        prepared_version: plan.prepared_version,
        suggested_tag: plan.suggested_tag,
        tag,
        version_override_used: plan.version_override_used,
        release_date: plan.release_date,
        prepared_at: plan.prepared_at,
        prepared_branch: plan.prepared_branch,
        prepared_head: plan.prepared_head,
        branch,
        current_head: plan.current_head,
        remote,
        commit_message,
        commit_sha,
        stale: plan.stale,
        stale_override_used: plan.stale_override_used,
        fingerprint_drift: plan.fingerprint_drift,
        warnings: plan.warnings,
        blockers,
        files_committed,
        state_file_removed,
        committed,
        tag_created,
        pushed,
        executed,
        post_release_instructions,
    })
}

pub fn render_prepared_changelog_contents(
    parsed: &changelog::Changelog,
    next_version: &semver::Version,
    release_date: &str,
) -> Result<String, ReleaseError> {
    let Some(unreleased_index) = parsed
        .releases
        .iter()
        .position(|release| release.is_unreleased())
    else {
        return Err(ReleaseError::TaskInvocation(
            "changelog is missing `## [Unreleased]`".to_owned(),
        ));
    };
    let mut updated = parsed.clone();
    let unreleased_categories = updated.releases[unreleased_index].categories.clone();
    updated.releases[unreleased_index].categories.clear();
    updated.releases.insert(
        unreleased_index + 1,
        changelog::Release {
            version: Some(next_version.clone()),
            date: Some(release_date.to_owned()),
            categories: unreleased_categories,
            line: 0,
        },
    );
    Ok(changelog::format(&updated))
}

fn unreleased_counts(changelog: &changelog::Changelog) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    if let Some(unreleased) = changelog.unreleased() {
        for category in &unreleased.categories {
            let count = category.entries.len();
            if count > 0 {
                counts.insert(category.kind.to_string(), count);
            }
        }
    }
    counts
}

pub fn suggested_bump(
    changelog: &changelog::Changelog,
    current_version: &semver::Version,
    pre_1_0: bool,
) -> BumpKind {
    let Some(unreleased) = changelog.unreleased() else {
        return BumpKind::None;
    };

    let mut has_breaking = false;
    let mut has_minor = false;
    let mut has_patch = false;

    for category in &unreleased.categories {
        if category.entries.is_empty() {
            continue;
        }
        match category.kind {
            CategoryKind::Breaking => has_breaking = true,
            CategoryKind::Added
            | CategoryKind::Changed
            | CategoryKind::Deprecated
            | CategoryKind::Removed => has_minor = true,
            CategoryKind::Fixed | CategoryKind::Security => has_patch = true,
        }
    }

    if !has_breaking && !has_minor && !has_patch {
        return BumpKind::None;
    }
    if has_breaking {
        if current_version.major == 0 && pre_1_0 {
            return BumpKind::Minor;
        }
        return BumpKind::Major;
    }
    if has_minor {
        return if current_version.major == 0 {
            BumpKind::Patch
        } else {
            BumpKind::Minor
        };
    }
    BumpKind::Patch
}

fn apply_bump(version: &semver::Version, bump: BumpKind) -> Option<semver::Version> {
    match bump {
        BumpKind::Major => Some(semver::Version::new(version.major + 1, 0, 0)),
        BumpKind::Minor => Some(semver::Version::new(version.major, version.minor + 1, 0)),
        BumpKind::Patch => Some(semver::Version::new(
            version.major,
            version.minor,
            version.patch + 1,
        )),
        BumpKind::None => None,
    }
}

fn build_sync_mutations(sync_files: &[ResolvedSyncFile]) -> Vec<FileMutationPlan> {
    sync_files
        .iter()
        .map(|sync| match sync.kind {
            SyncFileKind::CargoLock => FileMutationPlan {
                path: sync.path.clone(),
                kind: "sync-file",
                summary: "sync Cargo.lock via `cargo generate-lockfile --quiet`".to_owned(),
                before_preview: if sync.path.exists() {
                    "Cargo.lock exists and will be regenerated".to_owned()
                } else {
                    "Cargo.lock is missing and will be created".to_owned()
                },
                after_preview: "Cargo.lock synced via `cargo generate-lockfile --quiet`".to_owned(),
                detail_lines: vec![
                    "sync command: cargo generate-lockfile --quiet".to_owned(),
                    "preview fidelity: lockfile contents are generated at apply time".to_owned(),
                ],
                diff_preview: Vec::new(),
                apply: FileMutationApply::SyncCargoLock,
            },
        })
        .collect()
}

pub fn gate_blockers(results: &[GateResult]) -> Vec<String> {
    results
        .iter()
        .filter(|gate| !gate.passed)
        .map(|gate| format!("gate `{}` failed", gate.name))
        .collect()
}

pub fn gate_blockers_if_checked(check_gates: bool, results: &[GateResult]) -> Vec<String> {
    if check_gates {
        gate_blockers(results)
    } else {
        Vec::new()
    }
}

pub fn snapshot_mutation_paths(
    mutations: &[FileMutationPlan],
) -> Result<BTreeMap<PathBuf, Option<Vec<u8>>>, ReleaseError> {
    let mut snapshots = BTreeMap::new();
    for mutation in mutations {
        if snapshots.contains_key(&mutation.path) {
            continue;
        }
        let current = match std::fs::read(&mutation.path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(ReleaseError::TaskInvocation(format!(
                    "failed to snapshot planned release file {}: {error}",
                    mutation.path.display()
                )));
            }
        };
        snapshots.insert(mutation.path.clone(), current);
    }
    Ok(snapshots)
}

pub fn collect_changed_mutation_paths(
    mutations: &[FileMutationPlan],
    snapshots: &BTreeMap<PathBuf, Option<Vec<u8>>>,
) -> Result<Vec<PathBuf>, ReleaseError> {
    let mut changed = Vec::new();
    let mut seen = BTreeSet::new();
    for mutation in mutations {
        if !seen.insert(mutation.path.clone()) {
            continue;
        }
        let before = snapshots.get(&mutation.path).ok_or_else(|| {
            ReleaseError::TaskInvocation(format!(
                "missing pre-apply snapshot for planned release file {}",
                mutation.path.display()
            ))
        })?;
        let after = match std::fs::read(&mutation.path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(ReleaseError::TaskInvocation(format!(
                    "failed to inspect planned release file {} after mutation: {error}",
                    mutation.path.display()
                )));
            }
        };
        if before.as_ref() != after.as_ref() {
            changed.push(mutation.path.clone());
        }
    }
    Ok(changed)
}

pub fn apply_release_mutations(
    root: &Path,
    mutations: &[FileMutationPlan],
) -> Result<(), ReleaseError> {
    for mutation in mutations {
        match &mutation.apply {
            FileMutationApply::Write { after_contents } => {
                std::fs::write(&mutation.path, after_contents).map_err(|error| {
                    ReleaseError::TaskInvocation(format!(
                        "failed to write {}: {error}",
                        mutation.path.display()
                    ))
                })?;
            }
            FileMutationApply::SyncCargoLock => sync_cargo_lock(root, &mutation.path)?,
        }
    }
    Ok(())
}

pub fn write_release_prepared_state(
    path: &Path,
    repo_root: &Path,
    previous_version: &semver::Version,
    suggested_version: Option<&semver::Version>,
    prepared_version: Option<&semver::Version>,
    suggested_tag: Option<&str>,
    tag: Option<&str>,
    version_override_used: bool,
    release_date: &str,
    gates_checked: bool,
    files_modified: &[PathBuf],
    prepared_branch: Option<&str>,
    prepared_head: Option<&str>,
) -> Result<(), ReleaseError> {
    let source_fingerprints = capture_release_prepared_source_fingerprints(
        repo_root,
        files_modified,
        prepared_branch,
        prepared_head,
    )?;
    let payload = json!({
        "schema": "effigy.release.prepared.v1",
        "schema_version": 2,
        "previous_version": previous_version.to_string(),
        "suggested_version": suggested_version.map(ToString::to_string),
        "version": prepared_version.map(ToString::to_string),
        "suggested_tag": suggested_tag,
        "tag": tag,
        "version_override_used": version_override_used,
        "release_date": release_date,
        "prepared_at": Utc::now().to_rfc3339(),
        "gates_checked": gates_checked,
        "gates_passed": true,
        "files_modified": files_modified
            .iter()
            .map(|value| value.display().to_string())
            .collect::<Vec<_>>(),
        "source_fingerprints": {
            "prepared_branch": source_fingerprints.prepared_branch,
            "prepared_head": source_fingerprints.prepared_head,
            "files": source_fingerprints
                .files
                .iter()
                .map(|fingerprint| {
                    json!({
                        "path": fingerprint.path.display().to_string(),
                        "digest": fingerprint.digest,
                    })
                })
                .collect::<Vec<_>>(),
        },
    });
    let rendered = serde_json::to_string_pretty(&payload).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to render release state file {}: {error}",
            path.display()
        ))
    })?;
    std::fs::write(path, rendered).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to write release state file {}: {error}",
            path.display()
        ))
    })
}

pub fn load_release_prepared_state(path: &Path) -> Result<ReleasePreparedState, ReleaseError> {
    let raw = std::fs::read_to_string(path).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to read release state file {}: {error}",
            path.display()
        ))
    })?;
    let parsed: RawReleasePreparedState = serde_json::from_str(&raw).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to parse release state file {}: {error}",
            path.display()
        ))
    })?;

    if parsed.schema != "effigy.release.prepared.v1" {
        return Err(ReleaseError::TaskInvocation(format!(
            "release state file {} uses unsupported schema `{}`",
            path.display(),
            parsed.schema
        )));
    }

    let previous_version = semver::Version::parse(&parsed.previous_version).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "release state file {} has invalid previous_version `{}`: {error}",
            path.display(),
            parsed.previous_version
        ))
    })?;
    let prepared_version = semver::Version::parse(parsed.version.as_deref().ok_or_else(|| {
        ReleaseError::TaskInvocation(format!(
            "release state file {} is missing a prepared `version` value",
            path.display()
        ))
    })?)
    .map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "release state file {} has invalid prepared version: {error}",
            path.display()
        ))
    })?;
    let suggested_version = parsed
        .suggested_version
        .as_deref()
        .map(semver::Version::parse)
        .transpose()
        .map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "release state file {} has invalid suggested_version: {error}",
                path.display()
            ))
        })?;
    let prepared_at = DateTime::parse_from_rfc3339(&parsed.prepared_at)
        .map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "release state file {} has invalid prepared_at `{}`: {error}",
                path.display(),
                parsed.prepared_at
            ))
        })?
        .with_timezone(&Utc);

    Ok(ReleasePreparedState {
        previous_version,
        suggested_version,
        prepared_version,
        suggested_tag: parsed.suggested_tag,
        tag: parsed.tag,
        version_override_used: parsed.version_override_used.unwrap_or(false),
        release_date: parsed.release_date,
        prepared_at,
        prepared_at_raw: parsed.prepared_at,
        gates_checked: parsed.gates_checked.unwrap_or(false),
        gates_passed: parsed.gates_passed.unwrap_or(false),
        files_modified: parsed
            .files_modified
            .into_iter()
            .map(PathBuf::from)
            .collect(),
        source_fingerprints: parsed.source_fingerprints.map(|fingerprints| {
            ReleasePreparedSourceFingerprints {
                prepared_branch: fingerprints.prepared_branch,
                prepared_head: fingerprints.prepared_head,
                files: fingerprints
                    .files
                    .into_iter()
                    .map(|file| ReleasePreparedFileFingerprint {
                        path: PathBuf::from(file.path),
                        digest: file.digest,
                    })
                    .collect(),
            }
        }),
    })
}

pub fn normalized_expected_files(
    state_file_name: &str,
    repo_root: &Path,
    files: &[PathBuf],
) -> Vec<String> {
    let mut normalized = files
        .iter()
        .map(|path| normalize_repo_relative_path(repo_root, path))
        .collect::<BTreeSet<_>>();
    normalized.insert(state_file_name.to_owned());
    normalized.into_iter().collect()
}

pub fn normalized_repo_files(repo_root: &Path, files: &[PathBuf]) -> Vec<String> {
    files
        .iter()
        .map(|path| normalize_repo_relative_path(repo_root, path))
        .collect()
}

pub fn compare_release_state_fingerprints(
    repo_root: &Path,
    fingerprints: &ReleasePreparedSourceFingerprints,
    current_branch: Option<&str>,
    current_head: Option<&str>,
) -> Vec<String> {
    let mut drift = Vec::new();

    if let (Some(prepared_branch), Some(current_branch)) =
        (fingerprints.prepared_branch.as_deref(), current_branch)
    {
        if prepared_branch != current_branch {
            drift.push(format!(
                "current branch `{current_branch}` differs from prepared branch `{prepared_branch}`"
            ));
        }
    }

    if let (Some(prepared_head), Some(current_head)) =
        (fingerprints.prepared_head.as_deref(), current_head)
    {
        if prepared_head != current_head {
            drift.push(format!(
                "HEAD moved since prepare: prepared `{prepared_head}`, current `{current_head}`"
            ));
        }
    }

    for file in &fingerprints.files {
        let absolute = repo_root.join(&file.path);
        match std::fs::read(&absolute) {
            Ok(body) => {
                let digest = release_digest_hex(&body);
                if digest != file.digest {
                    drift.push(format!(
                        "prepared file content drifted since prepare: {}",
                        file.path.display()
                    ));
                }
            }
            Err(error) => drift.push(format!(
                "prepared file became unreadable since prepare: {} ({error})",
                file.path.display()
            )),
        }
    }

    drift
}

fn sync_cargo_lock(root: &Path, lockfile: &Path) -> Result<(), ReleaseError> {
    let output = ProcessCommand::new("cargo")
        .arg("generate-lockfile")
        .arg("--quiet")
        .current_dir(root)
        .output()
        .map_err(|error| {
            ReleaseError::TaskInvocation(format!("failed to sync {}: {error}", lockfile.display()))
        })?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        "cargo generate-lockfile --quiet exited unsuccessfully".to_owned()
    };
    Err(ReleaseError::TaskInvocation(format!(
        "failed to sync {}: {detail}",
        lockfile.display()
    )))
}

fn capture_release_prepared_source_fingerprints(
    repo_root: &Path,
    files_modified: &[PathBuf],
    prepared_branch: Option<&str>,
    prepared_head: Option<&str>,
) -> Result<ReleasePreparedSourceFingerprints, ReleaseError> {
    let files = files_modified
        .iter()
        .map(|path| {
            let relative = normalize_repo_relative_path(repo_root, path);
            let absolute = if path.is_absolute() {
                path.to_path_buf()
            } else {
                repo_root.join(path)
            };
            let body = std::fs::read(&absolute).map_err(|error| {
                ReleaseError::TaskInvocation(format!(
                    "failed to read release source file {}: {error}",
                    absolute.display()
                ))
            })?;
            Ok(ReleasePreparedFileFingerprint {
                path: PathBuf::from(relative),
                digest: release_digest_hex(&body),
            })
        })
        .collect::<Result<Vec<_>, ReleaseError>>()?;

    Ok(ReleasePreparedSourceFingerprints {
        prepared_branch: prepared_branch.map(str::to_owned),
        prepared_head: prepared_head.map(str::to_owned),
        files,
    })
}

fn normalize_repo_relative_path(repo_root: &Path, path: &Path) -> String {
    if path.is_absolute() {
        path.strip_prefix(repo_root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string()
    } else {
        path.to_string_lossy().to_string()
    }
}

fn release_digest_hex(bytes: &[u8]) -> String {
    let mut state: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(0x100000001b3);
    }
    format!("{state:016x}")
}

fn build_post_release_instructions(tag: Option<&str>) -> Vec<String> {
    let mut instructions = vec![
        "Confirm the release CI pipeline starts for the pushed branch and tag.".to_owned(),
        "Monitor the published release artifacts before announcing availability.".to_owned(),
    ];
    if let Some(tag) = tag {
        instructions.push(format!(
            "Verify the remote tag `{tag}` points at the release commit."
        ));
    }
    instructions
}

pub fn resolve_verify_install_tag(
    tag: Option<String>,
    github_ref_name: Option<String>,
) -> Result<String, ReleaseError> {
    tag.or(github_ref_name)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ReleaseError::TaskInvocation(
                "release verify-install requires `--tag <TAG>` or `GITHUB_REF_NAME`".to_owned(),
            )
        })
}

pub fn normalize_verify_install_repo_url(repo_url: &str) -> String {
    let trimmed = repo_url.trim();
    if trimmed.is_empty()
        || trimmed.contains("://")
        || trimmed.starts_with('/')
        || trimmed.starts_with("./")
        || trimmed.starts_with("../")
        || trimmed.starts_with("~/")
    {
        return trimmed.to_owned();
    }

    if let Some((host_part, path_part)) = trimmed.split_once(':') {
        if !path_part.is_empty()
            && path_part.contains('/')
            && !path_part.starts_with('/')
            && (host_part.contains('@') || host_part.contains('.'))
        {
            return format!("ssh://{host_part}/{}", path_part.trim_start_matches('/'));
        }
    }

    trimmed.to_owned()
}

fn make_release_temp_dir(purpose: &str) -> Result<PathBuf, ReleaseError> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| {
            ReleaseError::TaskInvocation(format!("failed to read system time: {error}"))
        })?
        .as_nanos();
    let root = std::env::temp_dir().join(format!("effigy-release-{purpose}-{ts}"));
    std::fs::create_dir_all(&root).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to create release temp directory `{}`: {error}",
            root.display()
        ))
    })?;
    Ok(root)
}

fn write_release_install_fixture(path: &Path) -> Result<(), ReleaseError> {
    let manifest_path = path.join("effigy.toml");
    std::fs::write(
        &manifest_path,
        "[catalog]\nalias = \"catalog_a\"\n\n[tasks]\nnoop = \"echo noop\"\n",
    )
    .map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to write verify-install fixture `{}`: {error}",
            manifest_path.display()
        ))
    })
}

fn run_verification_step(
    name: &str,
    program: &str,
    args: &[String],
    cwd: Option<&Path>,
) -> VerificationStepResult {
    let mut command = ProcessCommand::new(program);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let started = Instant::now();
    match command.output() {
        Ok(output) => VerificationStepResult {
            name: name.to_owned(),
            command: format_command(program, args),
            passed: output.status.success(),
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            launch_error: None,
            duration_ms: started.elapsed().as_millis(),
        },
        Err(error) => VerificationStepResult {
            name: name.to_owned(),
            command: format_command(program, args),
            passed: false,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            launch_error: Some(error.to_string()),
            duration_ms: started.elapsed().as_millis(),
        },
    }
}

fn format_command(program: &str, args: &[String]) -> String {
    if args.is_empty() {
        return program.to_owned();
    }
    format!("{program} {}", args.join(" "))
}

pub fn run_release_verify_install(
    repo_root: PathBuf,
    tag: String,
    repo_url: String,
) -> Result<ReleaseVerifyInstall, ReleaseError> {
    let temp_root = make_release_temp_dir("verify-install")?;
    let install_root = temp_root.join("install-root");
    let fixture_dir = temp_root.join("fixture");
    std::fs::create_dir_all(&fixture_dir).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to create verify-install fixture directory `{}`: {error}",
            fixture_dir.display()
        ))
    })?;
    write_release_install_fixture(&fixture_dir)?;

    let install_command = vec![
        "install".to_owned(),
        "--git".to_owned(),
        repo_url.clone(),
        "--tag".to_owned(),
        tag.clone(),
        "--root".to_owned(),
        install_root.display().to_string(),
        "--force".to_owned(),
        "effigy".to_owned(),
    ];
    let mut results = vec![run_verification_step(
        "cargo install from git tag",
        "cargo",
        &install_command,
        None,
    )];

    let mut blockers = Vec::new();
    if !results[0].passed {
        blockers.push(format!(
            "install verification step `{}` failed",
            results[0].name
        ));
        return Ok(ReleaseVerifyInstall {
            repo_root,
            tag,
            repo_url,
            installed_bin: None,
            configured_check_count: 7,
            executed_check_count: results.len(),
            stopped_early: true,
            results,
            blockers,
            verified: false,
        });
    }

    let installed_bin = install_root.join("bin/effigy");
    if !installed_bin.is_file() {
        blockers.push(format!(
            "installed binary is missing or not executable: {}",
            installed_bin.display()
        ));
        return Ok(ReleaseVerifyInstall {
            repo_root,
            tag,
            repo_url,
            installed_bin: Some(installed_bin),
            configured_check_count: 7,
            executed_check_count: results.len(),
            stopped_early: true,
            results,
            blockers,
            verified: false,
        });
    }

    let verification_checks = vec![
        (
            "installed binary help",
            installed_bin.clone(),
            vec!["help".to_owned()],
        ),
        (
            "installed binary tasks fixture check",
            installed_bin.clone(),
            vec![
                "tasks".to_owned(),
                "--repo".to_owned(),
                fixture_dir.display().to_string(),
            ],
        ),
        (
            "installed binary prefixed builtin tasks check",
            installed_bin.clone(),
            vec![
                "catalog_a/tasks".to_owned(),
                "--repo".to_owned(),
                fixture_dir.display().to_string(),
            ],
        ),
        (
            "installed binary json help check",
            installed_bin.clone(),
            vec!["--json".to_owned(), "help".to_owned()],
        ),
        (
            "installed binary completion check",
            installed_bin.clone(),
            vec!["completion".to_owned(), "bash".to_owned()],
        ),
        (
            "installed binary completion candidates check",
            installed_bin.clone(),
            vec![
                "completion".to_owned(),
                "candidates".to_owned(),
                "--repo".to_owned(),
                fixture_dir.display().to_string(),
            ],
        ),
    ];

    let mut stopped_early = false;
    for (name, program, args) in verification_checks {
        let result = run_verification_step(name, &program.display().to_string(), &args, None);
        let passed = result.passed;
        results.push(result);
        if !passed {
            blockers.push(format!(
                "install verification step `{}` failed",
                results
                    .last()
                    .map(|step| step.name.as_str())
                    .unwrap_or(name)
            ));
            stopped_early = true;
            break;
        }
    }

    Ok(ReleaseVerifyInstall {
        repo_root,
        tag,
        repo_url,
        installed_bin: Some(installed_bin),
        configured_check_count: 7,
        executed_check_count: results.len(),
        stopped_early,
        blockers: blockers.clone(),
        verified: blockers.is_empty(),
        results,
    })
}

#[cfg(test)]
mod tests;
