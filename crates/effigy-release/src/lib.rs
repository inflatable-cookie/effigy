use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::time::Instant;

use chrono::{DateTime, Utc};
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

pub fn detect_version_file_kind(path: &Path) -> Option<VersionFileKind> {
    match path.file_name().and_then(|name| name.to_str()) {
        Some("Cargo.toml") => Some(VersionFileKind::CargoToml),
        Some("package.json") => Some(VersionFileKind::PackageJson),
        Some("pyproject.toml") => Some(VersionFileKind::PyProjectToml),
        Some("VERSION") => Some(VersionFileKind::PlainText),
        _ => None,
    }
}

pub fn resolve_version_field_path(
    kind: VersionFileKind,
    configured: Option<&str>,
) -> Result<Option<String>, ReleaseError> {
    if let Some(configured) = configured {
        let trimmed = configured.trim();
        if trimmed.is_empty() {
            return Err(ReleaseError::TaskInvocation(
                "release.version-path must not be empty".to_owned(),
            ));
        }
        if matches!(kind, VersionFileKind::PlainText) {
            return Err(ReleaseError::TaskInvocation(
                "release.version-path is not supported for VERSION files".to_owned(),
            ));
        }
        return Ok(Some(trimmed.to_owned()));
    }

    Ok(match kind {
        VersionFileKind::CargoToml => Some("package.version".to_owned()),
        VersionFileKind::PackageJson => Some("version".to_owned()),
        VersionFileKind::PyProjectToml => None,
        VersionFileKind::PlainText => None,
    })
}

pub fn format_release_tag(tag_format: &str, version: &semver::Version) -> String {
    tag_format.replace("{version}", &version.to_string())
}

pub fn run_release_gates(
    root: &Path,
    gates: &[ResolvedGate],
    fail_fast: bool,
) -> GateExecutionReport {
    let started = Instant::now();
    let mut results = Vec::with_capacity(gates.len());
    let mut stopped_early = false;

    for gate in gates {
        let result = run_release_gate(root, gate);
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

pub fn render_release_status_json(status: &ReleaseStatus) -> String {
    let gates_json = gate_results_json(&status.gate_results);
    serde_json::to_string_pretty(&json!({
        "schema": "effigy.release.status.v1",
        "schema_version": 1,
        "ready": status.ready,
        "repo_root": status.repo_root.display().to_string(),
        "current_version": status.current_version.to_string(),
        "version_source": {
            "file": status.version_source.path.display().to_string(),
            "format": status.version_source.kind.format_label(),
            "path": status.version_source.field_path.clone(),
        },
        "changelog": {
            "path": status.changelog_path.display().to_string(),
            "valid": status.changelog_valid,
            "diagnostic_count": status.changelog_diagnostics.len(),
            "diagnostics": status.changelog_diagnostics.clone(),
        },
        "unreleased": {
            "empty": status.unreleased_empty,
            "entry_count": status.unreleased_counts.values().copied().sum::<usize>(),
            "counts": status.unreleased_counts.clone(),
        },
        "suggested_bump": status.suggested_bump,
        "next_version": status.next_version.as_ref().map(ToString::to_string),
        "tag": status.tag.clone(),
        "gates": {
            "checked": status.gates_checked,
            "configured_count": status.configured_gate_count,
            "results": gates_json,
        },
        "blockers": status.blockers.clone(),
    }))
    .unwrap_or_else(|_| "{\"schema\":\"effigy.release.status.v1\",\"ready\":false}".to_owned())
}

pub fn render_release_gate_run_json(run: &ReleaseGateRun) -> String {
    serde_json::to_string_pretty(&json!({
        "schema": "effigy.release.gates.v1",
        "schema_version": 1,
        "passed": run.passed,
        "repo_root": run.repo_root.display().to_string(),
        "configured_gate_count": run.configured_gate_count,
        "executed_gate_count": run.executed_gate_count,
        "stopped_early": run.stopped_early,
        "total_duration_ms": run.total_duration_ms,
        "results": gate_results_json(&run.gate_results),
        "blockers": run.blockers.clone(),
    }))
    .unwrap_or_else(|_| "{\"schema\":\"effigy.release.gates.v1\",\"passed\":false}".to_owned())
}

pub fn render_release_verify_install_json(result: &ReleaseVerifyInstall) -> String {
    serde_json::to_string_pretty(&json!({
        "schema": "effigy.release.verify-install.v1",
        "schema_version": 1,
        "verified": result.verified,
        "repo_root": result.repo_root.display().to_string(),
        "tag": result.tag,
        "repo_url": result.repo_url,
        "installed_bin": result.installed_bin.as_ref().map(|path| path.display().to_string()),
        "configured_check_count": result.configured_check_count,
        "executed_check_count": result.executed_check_count,
        "stopped_early": result.stopped_early,
        "results": verification_results_json(&result.results),
        "blockers": result.blockers.clone(),
    }))
    .unwrap_or_else(|_| {
        "{\"schema\":\"effigy.release.verify-install.v1\",\"verified\":false}".to_owned()
    })
}

pub fn render_release_prepare_plan_json(plan: &ReleasePreparePlan) -> String {
    let gates_json = gate_results_json(&plan.gate_results);
    let mutations_json = mutation_plans_json(&plan.mutations);
    serde_json::to_string_pretty(&json!({
        "schema": "effigy.release.prepare.plan.v1",
        "schema_version": 1,
        "mode": "plan",
        "ready": plan.ready,
        "repo_root": plan.repo_root.display().to_string(),
        "current_version": plan.current_version.to_string(),
        "version_source": {
            "file": plan.version_source.path.display().to_string(),
            "format": plan.version_source.kind.format_label(),
            "path": plan.version_source.field_path.clone(),
        },
        "suggested_version": plan.suggested_version.as_ref().map(ToString::to_string),
        "planned_version": plan.planned_version.as_ref().map(ToString::to_string),
        "suggested_tag": plan.suggested_tag.clone(),
        "tag": plan.tag.clone(),
        "version_override_used": plan.version_override_used,
        "release_date": plan.release_date,
        "gates": {
            "checked": plan.gates_checked,
            "configured_count": plan.configured_gate_count,
            "results": gates_json,
        },
        "mutations": mutations_json,
        "blockers": plan.blockers.clone(),
    }))
    .unwrap_or_else(|_| {
        "{\"schema\":\"effigy.release.prepare.plan.v1\",\"ready\":false}".to_owned()
    })
}

pub fn render_release_simulation_json(simulation: &ReleaseSimulation) -> String {
    let mutations_json = mutation_plans_json(&simulation.mutations);
    serde_json::to_string_pretty(&json!({
        "schema": "effigy.release.simulate.v1",
        "schema_version": 1,
        "mode": "simulate",
        "ready": simulation.ready,
        "repo_root": simulation.repo_root.display().to_string(),
        "current_version": simulation.current_version.to_string(),
        "version_source": {
            "file": simulation.version_source.path.display().to_string(),
            "format": simulation.version_source.kind.format_label(),
            "path": simulation.version_source.field_path.clone(),
        },
        "suggested_version": simulation.suggested_version.as_ref().map(ToString::to_string),
        "planned_version": simulation.planned_version.as_ref().map(ToString::to_string),
        "suggested_tag": simulation.suggested_tag.clone(),
        "tag": simulation.tag.clone(),
        "version_override_used": simulation.version_override_used,
        "release_date": simulation.release_date,
        "commit_message": simulation.commit_message.clone(),
        "state_file": simulation.state_file.display().to_string(),
        "state_file_exists": simulation.state_file_exists,
        "state_file_written": simulation.state_file_written,
        "gates": {
            "configured_count": simulation.configured_gate_count,
            "executed_count": simulation.executed_gate_count,
            "stopped_early": simulation.stopped_early,
            "total_duration_ms": simulation.total_duration_ms,
            "results": gate_results_json(&simulation.gate_results),
        },
        "mutations": mutations_json,
        "blockers": simulation.blockers.clone(),
    }))
    .unwrap_or_else(|_| "{\"schema\":\"effigy.release.simulate.v1\",\"ready\":false}".to_owned())
}

pub fn render_release_prepared_json(result: &ReleasePrepared) -> String {
    let gates_json = gate_results_json(&result.gate_results);
    serde_json::to_string_pretty(&json!({
        "schema": "effigy.release.prepare.v1",
        "schema_version": 1,
        "prepared": result.prepared,
        "repo_root": result.repo_root.display().to_string(),
        "previous_version": result.previous_version.to_string(),
        "suggested_version": result.suggested_version.as_ref().map(ToString::to_string),
        "prepared_version": result.prepared_version.as_ref().map(ToString::to_string),
        "suggested_tag": result.suggested_tag.clone(),
        "tag": result.tag.clone(),
        "version_override_used": result.version_override_used,
        "release_date": result.release_date,
        "state_file": result.state_file.display().to_string(),
        "state_file_written": result.state_file_written,
        "gates": {
            "checked": result.gates_checked,
            "configured_count": result.configured_gate_count,
            "results": gates_json,
        },
        "files_modified": result.files_modified.iter().map(|path| path.display().to_string()).collect::<Vec<_>>(),
        "blockers": result.blockers.clone(),
    }))
    .unwrap_or_else(|_| "{\"schema\":\"effigy.release.prepare.v1\",\"prepared\":false}".to_owned())
}

pub fn render_release_execute_plan_json(plan: &ReleaseExecutePlan) -> String {
    serde_json::to_string_pretty(&json!({
        "schema": "effigy.release.execute.plan.v1",
        "schema_version": 1,
        "mode": "plan",
        "ready": plan.ready,
        "repo_root": plan.repo_root.display().to_string(),
        "state_file": plan.state_file.display().to_string(),
        "state_loaded": plan.state_loaded,
        "previous_version": plan.previous_version.as_ref().map(ToString::to_string),
        "suggested_version": plan.suggested_version.as_ref().map(ToString::to_string),
        "prepared_version": plan.prepared_version.as_ref().map(ToString::to_string),
        "suggested_tag": plan.suggested_tag.clone(),
        "tag": plan.tag.clone(),
        "version_override_used": plan.version_override_used,
        "release_date": plan.release_date.clone(),
        "prepared_at": plan.prepared_at.clone(),
        "prepared_branch": plan.prepared_branch.clone(),
        "prepared_head": plan.prepared_head.clone(),
        "stale": plan.stale,
        "stale_threshold_seconds": plan.stale_threshold_seconds,
        "stale_override_required": plan.stale_override_required,
        "stale_override_used": plan.stale_override_used,
        "branch": plan.branch.clone(),
        "current_head": plan.current_head.clone(),
        "remote": plan.remote.clone(),
        "gates": {
            "checked": plan.gates_checked,
            "passed": plan.gates_passed,
        },
        "source_fingerprints": {
            "available": plan.source_fingerprint_available,
            "drift": plan.fingerprint_drift.clone(),
        },
        "working_tree": {
            "expected_files": plan.expected_files.clone(),
            "modified_files": plan.modified_files.clone(),
            "missing_expected_files": plan.missing_expected_files.clone(),
            "unexpected_files": plan.unexpected_files.clone(),
        },
        "warnings": plan.warnings.clone(),
        "blockers": plan.blockers.clone(),
    }))
    .unwrap_or_else(|_| {
        "{\"schema\":\"effigy.release.execute.plan.v1\",\"ready\":false}".to_owned()
    })
}

pub fn render_release_resume_json(
    plan: &ReleaseExecutePlan,
    suggested_actions: &[String],
) -> String {
    serde_json::to_string_pretty(&json!({
        "schema": "effigy.release.resume.v1",
        "schema_version": 1,
        "state_loaded": plan.state_loaded,
        "review_available": plan.state_loaded,
        "ready_to_execute": plan.ready,
        "repo_root": plan.repo_root.display().to_string(),
        "state_file": plan.state_file.display().to_string(),
        "previous_version": plan.previous_version.as_ref().map(ToString::to_string),
        "suggested_version": plan.suggested_version.as_ref().map(ToString::to_string),
        "prepared_version": plan.prepared_version.as_ref().map(ToString::to_string),
        "suggested_tag": plan.suggested_tag.clone(),
        "tag": plan.tag.clone(),
        "version_override_used": plan.version_override_used,
        "release_date": plan.release_date.clone(),
        "prepared_at": plan.prepared_at.clone(),
        "prepared_branch": plan.prepared_branch.clone(),
        "prepared_head": plan.prepared_head.clone(),
        "stale": plan.stale,
        "stale_override_required": plan.stale_override_required,
        "stale_override_used": plan.stale_override_used,
        "branch": plan.branch.clone(),
        "current_head": plan.current_head.clone(),
        "remote": plan.remote.clone(),
        "gates": {
            "checked": plan.gates_checked,
            "passed": plan.gates_passed,
        },
        "source_fingerprints": {
            "available": plan.source_fingerprint_available,
            "drift": plan.fingerprint_drift.clone(),
        },
        "drift": {
            "expected_files": plan.expected_files.clone(),
            "modified_files": plan.modified_files.clone(),
            "missing_expected_files": plan.missing_expected_files.clone(),
            "unexpected_files": plan.unexpected_files.clone(),
        },
        "warnings": plan.warnings.clone(),
        "blockers": plan.blockers.clone(),
        "suggested_actions": suggested_actions,
    }))
    .unwrap_or_else(|_| {
        "{\"schema\":\"effigy.release.resume.v1\",\"state_loaded\":false}".to_owned()
    })
}

pub fn render_release_executed_json(result: &ReleaseExecuted) -> String {
    serde_json::to_string_pretty(&json!({
        "schema": "effigy.release.execute.v1",
        "schema_version": 1,
        "executed": result.executed,
        "repo_root": result.repo_root.display().to_string(),
        "state_file": result.state_file.display().to_string(),
        "previous_version": result.previous_version.as_ref().map(ToString::to_string),
        "suggested_version": result.suggested_version.as_ref().map(ToString::to_string),
        "prepared_version": result.prepared_version.as_ref().map(ToString::to_string),
        "suggested_tag": result.suggested_tag.clone(),
        "tag": result.tag.clone(),
        "version_override_used": result.version_override_used,
        "branch": result.branch.clone(),
        "remote": result.remote.clone(),
        "release_date": result.release_date.clone(),
        "prepared_at": result.prepared_at.clone(),
        "prepared_branch": result.prepared_branch.clone(),
        "prepared_head": result.prepared_head.clone(),
        "commit_message": result.commit_message.clone(),
        "commit_sha": result.commit_sha.clone(),
        "current_head": result.current_head.clone(),
        "stale": result.stale,
        "stale_override_used": result.stale_override_used,
        "fingerprint_drift": result.fingerprint_drift.clone(),
        "committed": result.committed,
        "tag_created": result.tag_created,
        "pushed": result.pushed,
        "state_file_removed": result.state_file_removed,
        "files_committed": result.files_committed.clone(),
        "warnings": result.warnings.clone(),
        "blockers": result.blockers.clone(),
        "post_release_instructions": result.post_release_instructions.clone(),
    }))
    .unwrap_or_else(|_| "{\"schema\":\"effigy.release.execute.v1\",\"executed\":false}".to_owned())
}

fn gate_results_json(gate_results: &[GateResult]) -> Vec<serde_json::Value> {
    gate_results
        .iter()
        .map(|gate| {
            json!({
                "name": gate.name,
                "description": gate.description,
                "command": gate.command,
                "passed": gate.passed,
                "exit_code": gate.exit_code,
                "stdout": gate.stdout,
                "stderr": gate.stderr,
                "launch_error": gate.launch_error,
                "duration_ms": gate.duration_ms,
            })
        })
        .collect()
}

fn verification_results_json(results: &[VerificationStepResult]) -> Vec<serde_json::Value> {
    results
        .iter()
        .map(|step| {
            json!({
                "name": step.name,
                "command": step.command,
                "passed": step.passed,
                "exit_code": step.exit_code,
                "stdout": step.stdout,
                "stderr": step.stderr,
                "launch_error": step.launch_error,
                "duration_ms": step.duration_ms,
            })
        })
        .collect()
}

fn mutation_plans_json(mutations: &[FileMutationPlan]) -> Vec<serde_json::Value> {
    mutations
        .iter()
        .map(|mutation| {
            json!({
                "path": mutation.path.display().to_string(),
                "kind": mutation.kind,
                "summary": mutation.summary,
                "before_preview": mutation.before_preview,
                "after_preview": mutation.after_preview,
                "detail_lines": mutation.detail_lines.clone(),
                "diff_preview": mutation.diff_preview.clone(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
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

        let error =
            load_release_config(&root).expect_err("Cargo.lock should be rejected for VERSION");
        assert!(error.to_string().contains(
            "`Cargo.lock` is only supported when the release version file is Cargo.toml"
        ));
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
}
