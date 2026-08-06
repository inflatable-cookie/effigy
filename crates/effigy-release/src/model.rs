use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use effigy_changelog::BumpKind;
use effigy_manifest::ManifestError;

use crate::{ResolvedGate, ResolvedSyncFile, ResolvedVersionSource};

#[derive(Debug, Clone)]
pub struct ReleaseConfig {
    pub version_source: ResolvedVersionSource,
    pub changelog_path: PathBuf,
    pub pre_1_0: bool,
    pub initial_tag_current_version: bool,
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

impl GateExecutionReport {
    pub fn empty() -> Self {
        Self {
            results: Vec::new(),
            stopped_early: false,
            total_duration_ms: 0,
        }
    }
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
    pub parsed_changelog: effigy_changelog::Changelog,
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
