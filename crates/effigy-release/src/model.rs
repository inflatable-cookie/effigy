use std::collections::BTreeMap;
use std::path::PathBuf;

use effigy_changelog::BumpKind;
use effigy_manifest::ManifestError;

use crate::hosted_evidence::HostedEvidenceSpec;
use crate::{
    ReleasePipelinePlan, ReleaseRunSummary, ResolvedGate, ResolvedSyncFile, ResolvedVersionSource,
};

#[derive(Debug, Clone)]
pub struct ReleaseConfig {
    pub version_source: ResolvedVersionSource,
    pub changelog_path: PathBuf,
    pub pre_1_0: bool,
    pub initial_tag_current_version: bool,
    pub sync_files: Vec<ResolvedSyncFile>,
    pub gates: Vec<ResolvedGate>,
    pub tag_format: String,
    pub hosted_evidence: Option<HostedEvidenceSpec>,
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
    pub log_path: Option<PathBuf>,
    pub reused: bool,
    pub hosted_run_url: Option<String>,
    pub hosted_head_sha: Option<String>,
    pub hosted_repository: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GateExecutionReport {
    pub results: Vec<GateResult>,
    pub stopped_early: bool,
    pub total_duration_ms: u128,
    pub environment_path: Option<PathBuf>,
}

impl GateExecutionReport {
    pub fn empty() -> Self {
        Self {
            results: Vec::new(),
            stopped_early: false,
            total_duration_ms: 0,
            environment_path: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReleaseStatus {
    pub repo_root: PathBuf,
    /// The resolved pipeline, for the identity, gate, artifact, target and
    /// approval sections.
    pub plan: ReleasePipelinePlan,
    /// The active or last release Run; no Run exists before the Run pipeline.
    pub run: ReleaseRunSummary,
    pub current_version: Option<semver::Version>,
    pub version_source: Option<ResolvedVersionSource>,
    pub changelog_path: PathBuf,
    pub changelog_valid: bool,
    pub changelog_diagnostics: Vec<String>,
    pub unreleased_counts: BTreeMap<String, usize>,
    pub unreleased_empty: bool,
    pub suggested_bump: String,
    pub next_version: Option<semver::Version>,
    pub tag: Option<String>,
    pub gates_checked: bool,
    pub gates_passed: bool,
    pub configured_gate_count: usize,
    pub gate_results: Vec<GateResult>,
    pub environment_path: Option<PathBuf>,
    pub blockers: Vec<String>,
    pub ready: bool,
}

#[derive(Debug, Clone)]
pub struct ReleaseGateRun {
    pub repo_root: PathBuf,
    /// The configured gate inventory, so a gate run reports what it intended to
    /// execute and not only what it managed to start.
    pub gates: Vec<ResolvedGate>,
    pub configured_gate_count: usize,
    pub executed_gate_count: usize,
    pub stopped_early: bool,
    pub total_duration_ms: u128,
    pub gate_results: Vec<GateResult>,
    pub environment_path: Option<PathBuf>,
    pub blockers: Vec<String>,
    pub passed: bool,
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
    Write {
        after_contents: String,
    },
    /// Refresh the lockfile's record of the workspace members' own versions
    /// after the version bump. `workspace_version` is the version the bump
    /// wrote, and the sync refuses to produce a lockfile carrying anything else
    /// (see `sync_cargo_lock`).
    SyncCargoLock {
        workspace_version: String,
    },
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
