//! Release pipeline resolution (contract 054 §2, §4 and §5).
//!
//! One `[release]` table resolves to a pipeline: identities, gates by stage,
//! artifacts, targets and approval checkpoints. `effigy release status` and
//! `effigy release --plan` render that pipeline and mutate nothing (R2).
//!
//! This module never writes: it reads the manifest, the changelog, the version
//! files and git state, and it runs no gate, task or publish.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use effigy_changelog::{self as changelog, BumpKind};
use effigy_manifest::config_sections::{
    ManifestReleaseArtifactConfig, ManifestReleaseConfig, ManifestReleaseGateConfig,
    ManifestReleaseGateDetails, ManifestReleaseGateStage, ManifestReleaseIdentityConfig,
    ManifestReleaseIdentityKind, ManifestReleaseTargetConfig, ManifestReleaseTargetKind,
};
use effigy_manifest::{load_task_manifest, TASK_MANIFEST_FILE};

use crate::hosted_evidence::{hosted_evidence_spec_from_manifest, HostedEvidenceSpec};
use crate::prepare_helpers::{apply_bump, suggested_bump, unreleased_counts};
use crate::{
    format_release_tag, git_head_sha, git_tag_exists, git_work_tree_clean, read_current_version,
    resolve_config_path, resolve_sync_files, resolve_version_source, ReleaseError,
    ResolvedSyncFile, ResolvedVersionSource,
};

/// The stage boundaries a Run can stop at for approval (contract 054 §3).
pub const RELEASE_STAGES: [&str; 7] = [
    "validate",
    "pre-gates",
    "prepare",
    "certify",
    "publish",
    "verify",
    "closeout",
];

/// The one stage boundary the §10 ruling 3 default approves.
const DEFAULT_APPROVAL_CHECKPOINT: &str = "publish";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityKind {
    Semver,
    Digest,
    Source,
    Task,
}

impl IdentityKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Semver => "semver",
            Self::Digest => "digest",
            Self::Source => "source",
            Self::Task => "task",
        }
    }

    fn from_manifest(kind: Option<ManifestReleaseIdentityKind>) -> Self {
        match kind {
            None | Some(ManifestReleaseIdentityKind::Semver) => Self::Semver,
            Some(ManifestReleaseIdentityKind::Digest) => Self::Digest,
            Some(ManifestReleaseIdentityKind::Source) => Self::Source,
            Some(ManifestReleaseIdentityKind::Task) => Self::Task,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateStage {
    Pre,
    Certify,
}

impl GateStage {
    /// The stage name this gate belongs to (`pre-gates` or `certify`).
    pub fn stage_name(self) -> &'static str {
        match self {
            Self::Pre => "pre-gates",
            Self::Certify => "certify",
        }
    }

    fn from_manifest(stage: Option<ManifestReleaseGateStage>) -> Self {
        match stage {
            // Contract 054 §4.1: the shorthand's gates run in certify.
            None | Some(ManifestReleaseGateStage::Certify) => Self::Certify,
            Some(ManifestReleaseGateStage::Pre) => Self::Pre,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedIdentity {
    pub name: String,
    pub kind: IdentityKind,
    pub version_source: Option<ResolvedVersionSource>,
    pub tag_format: Option<String>,
    pub follows: Option<String>,
    pub task: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedArtifact {
    pub name: String,
    pub task: String,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    Tag,
    Task,
}

impl TargetKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Tag => "tag",
            Self::Task => "task",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedTarget {
    pub name: String,
    pub kind: TargetKind,
    pub task: Option<String>,
    pub check: Option<String>,
    pub verify: Option<String>,
}

/// The resolved `[release]` table, before any version is read.
#[derive(Debug, Clone)]
pub struct ReleasePipeline {
    pub repo_root: PathBuf,
    pub single_identity_shorthand: bool,
    pub changelog_path: PathBuf,
    pub pre_1_0: bool,
    pub initial_tag_current_version: bool,
    pub sync_files: Vec<ResolvedSyncFile>,
    pub identities: Vec<ResolvedIdentity>,
    pub gates: Vec<crate::ResolvedGate>,
    pub artifacts: Vec<ResolvedArtifact>,
    pub targets: Vec<ResolvedTarget>,
    pub approvals: Vec<String>,
    pub approval_defaulted: bool,
    pub hosted_evidence: Option<HostedEvidenceSpec>,
}

impl ReleasePipeline {
    pub fn identity(&self, name: &str) -> Option<&ResolvedIdentity> {
        self.identities
            .iter()
            .find(|identity| identity.name == name)
    }
}

/// The active or last release Run, as `release status` reports it.
#[derive(Debug, Clone, Default)]
pub struct ReleaseRunSummary {
    pub id: Option<String>,
    pub stage: Option<String>,
    pub waiting_on: Option<String>,
    /// `paused`, `awaiting-approval`, `failed` or `complete`.
    pub status: Option<String>,
}

impl ReleaseRunSummary {
    pub fn none() -> Self {
        Self::default()
    }

    /// Whether the Run is unfinished: a complete release has no active Run.
    pub fn is_active(&self) -> bool {
        self.id.is_some() && self.status.as_deref() != Some("complete")
    }
}

/// One identity's resolved version, tag or non-semver identity value.
#[derive(Debug, Clone)]
pub struct ReleaseIdentityPlan {
    pub name: String,
    pub kind: IdentityKind,
    pub current_version: Option<semver::Version>,
    pub next_version: Option<semver::Version>,
    /// The commit SHA for `source`, and the value a `task` identity prints.
    /// `digest` identities have none until certify records the artifact.
    pub identity_value: Option<String>,
    pub bump: Option<String>,
    pub tag: Option<String>,
    pub follows: Option<String>,
    pub version_source: Option<ResolvedVersionSource>,
    /// The identity's own tag format, so a follower can re-render its tag from
    /// its leader's version.
    pub tag_format: Option<String>,
    pub task: Option<String>,
    pub files: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ReleaseArtifactPlan {
    pub name: String,
    pub task: String,
    pub path: String,
    /// The kind of the identity this artifact feeds, when it shares its name.
    pub identity_kind: Option<String>,
}

/// The whole pipeline with nothing mutated (contract 054 §5, `--plan`).
#[derive(Debug, Clone)]
pub struct ReleasePipelinePlan {
    pub repo_root: PathBuf,
    pub single_identity_shorthand: bool,
    pub changelog_path: PathBuf,
    /// Whether this release reads the changelog at all: true when it has a
    /// `semver` identity (contract 054 §4.3).
    pub changelog_in_use: bool,
    pub changelog_valid: bool,
    pub changelog_diagnostics: Vec<String>,
    pub unreleased_counts: BTreeMap<String, usize>,
    pub unreleased_empty: bool,
    /// Versions the changelog already records, for `--plan --version` and for
    /// reporting that a planned version is not new.
    pub existing_changelog_versions: Vec<String>,
    pub identities: Vec<ReleaseIdentityPlan>,
    pub gates: Vec<crate::ResolvedGate>,
    pub artifacts: Vec<ReleaseArtifactPlan>,
    pub targets: Vec<ResolvedTarget>,
    pub approvals: Vec<String>,
    pub approval_defaulted: bool,
    pub files: Vec<PathBuf>,
    /// `Some(true)` clean, `Some(false)` dirty, `None` not a git work tree.
    pub working_tree_clean: Option<bool>,
    pub blockers: Vec<String>,
    pub ready: bool,
}

impl ReleasePipelinePlan {
    /// The identity the pre-Run surfaces (`prepare`, `execute`, `status`'s
    /// single-version fields) report: the first `semver` identity.
    pub fn primary_identity(&self) -> Option<&ReleaseIdentityPlan> {
        self.identities
            .iter()
            .find(|identity| identity.kind == IdentityKind::Semver)
    }

    pub fn suggested_bump(&self) -> Option<&str> {
        self.primary_identity()
            .and_then(|identity| identity.bump.as_deref())
    }
}

fn has_semver_identity(identities: &[ResolvedIdentity]) -> bool {
    identities
        .iter()
        .any(|identity| identity.kind == IdentityKind::Semver)
}

/// Read `[release]` and resolve it to a pipeline. Reads only.
pub fn resolve_release_pipeline(root: &Path) -> Result<ReleasePipeline, ReleaseError> {
    let manifest_path = root.join(TASK_MANIFEST_FILE);
    let manifest = if manifest_path.exists() {
        Some(load_task_manifest(&manifest_path)?)
    } else {
        None
    };
    let config = manifest.as_ref().and_then(|parsed| parsed.release.as_ref());

    let single_identity_shorthand = config
        .map(|config| config.identities.is_empty())
        .unwrap_or(true);
    if single_identity_shorthand {
        if let Some(config) = config {
            reject_pipeline_keys_with_shorthand(config)?;
        }
    }

    let identities = resolve_identities(root, config, single_identity_shorthand)?;

    // Contract 054 §4.3: changelog bump rules apply only to `semver`
    // identities, so a release with none never reads — or requires — a
    // changelog. That is the §4.3 example's shape: no `changelog` key and no
    // file.
    let changelog_path = resolve_config_path(
        root,
        config.and_then(|config| config.changelog.as_deref()),
        "CHANGELOG.md",
        "release.changelog",
    )?;
    if has_semver_identity(&identities) && !changelog_path.exists() {
        return Err(ReleaseError::TaskInvocation(format!(
            "release changelog path does not exist: {}",
            changelog_path.display()
        )));
    }
    let gates = resolve_gates(config)?;
    let artifacts = resolve_artifacts(config);
    let targets = if single_identity_shorthand {
        // Contract 054 §4.1: the shorthand means one `tag` target.
        vec![ResolvedTarget {
            name: "tag".to_owned(),
            kind: TargetKind::Tag,
            task: None,
            check: None,
            verify: None,
        }]
    } else {
        resolve_targets(config)?
    };
    validate_pipeline_shape(config, &identities, &artifacts, &targets)?;

    let approvals = match config.and_then(|config| config.approve.clone()) {
        Some(checkpoints) => {
            for checkpoint in &checkpoints {
                let checkpoint = checkpoint.trim();
                if !RELEASE_STAGES.contains(&checkpoint) {
                    return Err(ReleaseError::TaskInvocation(format!(
                        "release.approve names `{checkpoint}`, which is not a stage boundary; approval checkpoints are {RELEASE_STAGES:?}"
                    )));
                }
            }
            (checkpoints, false)
        }
        None => {
            let has_non_tag_target = targets.iter().any(|target| target.kind != TargetKind::Tag);
            if has_non_tag_target {
                (vec![DEFAULT_APPROVAL_CHECKPOINT.to_owned()], true)
            } else {
                (Vec::new(), true)
            }
        }
    };

    let version_source = identities
        .iter()
        .find(|identity| identity.kind == IdentityKind::Semver)
        .and_then(|identity| identity.version_source.clone());
    let sync_files = match version_source.as_ref() {
        Some(source) => resolve_sync_files(root, config, source)?,
        None => {
            if config
                .map(|config| !config.sync_files.is_empty())
                .unwrap_or(false)
            {
                return Err(ReleaseError::TaskInvocation(
                    "release.sync-files needs a `semver` identity: there is no version for a lockfile or package manifest to carry".to_owned(),
                ));
            }
            Vec::new()
        }
    };

    Ok(ReleasePipeline {
        repo_root: root.to_path_buf(),
        single_identity_shorthand,
        changelog_path,
        pre_1_0: config.and_then(|config| config.pre_1_0).unwrap_or(true),
        initial_tag_current_version: config
            .and_then(|config| config.initial_tag_current_version)
            .unwrap_or(false),
        sync_files,
        identities,
        gates,
        artifacts,
        targets,
        approvals: approvals.0,
        approval_defaulted: approvals.1,
        hosted_evidence: hosted_evidence_spec_from_manifest(
            config.and_then(|config| config.hosted_evidence.as_ref()),
        )?,
    })
}

fn reject_pipeline_keys_with_shorthand(config: &ManifestReleaseConfig) -> Result<(), ReleaseError> {
    if config.artifacts.is_empty() && config.targets.is_empty() && config.approve.is_none() {
        return Ok(());
    }
    Err(ReleaseError::TaskInvocation(
        "release.artifacts, release.targets and release.approve need `[release.identities]`: the single-identity shorthand already means one identity, one `tag` target and no artifacts. Declare `[release.identities.<name>]` (for example `[release.identities.<repo>]` with the same `version-file`) to name the identity, then add the artifacts, targets and approval checkpoints".to_owned(),
    ))
}

fn resolve_identities(
    root: &Path,
    config: Option<&ManifestReleaseConfig>,
    single_identity_shorthand: bool,
) -> Result<Vec<ResolvedIdentity>, ReleaseError> {
    if single_identity_shorthand {
        let version_source = resolve_version_source(root, config)?;
        let tag_format = resolve_tag_format(config.and_then(|config| config.tag_format.as_deref()))
            .unwrap_or_else(|| "v{version}".to_owned());
        return Ok(vec![ResolvedIdentity {
            name: single_identity_name(root),
            kind: IdentityKind::Semver,
            version_source: Some(version_source),
            tag_format: Some(tag_format),
            follows: None,
            task: None,
        }]);
    }

    let config = config.expect("named identities require a [release] table");
    let mut identities = Vec::with_capacity(config.identities.len());
    for (name, identity) in &config.identities {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(ReleaseError::TaskInvocation(
                "release.identities keys must not be empty".to_owned(),
            ));
        }
        let kind = IdentityKind::from_manifest(identity.kind);
        let version_source = match kind {
            IdentityKind::Semver => Some(resolve_identity_version_source(root, trimmed, identity)?),
            _ => {
                if identity.version_file.is_some() || identity.version_path.is_some() {
                    return Err(ReleaseError::TaskInvocation(format!(
                        "release.identities.{trimmed} is kind `{}` and has no version file; remove `version-file`/`version-path` or set `kind = \"semver\"`",
                        kind.label()
                    )));
                }
                None
            }
        };
        let tag_format = match kind {
            IdentityKind::Semver => {
                let default = resolve_tag_format(config.tag_format.as_deref());
                let configured = resolve_tag_format(identity.tag_format.as_deref());
                let tag_format = configured.or(default).ok_or_else(|| {
                    ReleaseError::TaskInvocation(format!(
                        "release.identities.{trimmed} needs `tag-format` when the release has more than one identity: a shared default tag would collide across identities"
                    ))
                })?;
                Some(tag_format)
            }
            _ => {
                if identity.tag_format.is_some() {
                    return Err(ReleaseError::TaskInvocation(format!(
                        "release.identities.{trimmed} is kind `{}`; `tag-format` belongs to a `semver` identity",
                        kind.label()
                    )));
                }
                None
            }
        };
        if kind == IdentityKind::Task && identity.task.is_none() {
            return Err(ReleaseError::TaskInvocation(format!(
                "release.identities.{trimmed} is kind `task` and needs `task`: the task that prints the identity"
            )));
        }
        if kind != IdentityKind::Task && identity.task.is_some() {
            return Err(ReleaseError::TaskInvocation(format!(
                "release.identities.{trimmed} sets `task` but its kind is `{}`; `task` belongs to `kind = \"task\"`",
                kind.label()
            )));
        }
        identities.push(ResolvedIdentity {
            name: trimmed.to_owned(),
            kind,
            version_source,
            tag_format,
            follows: identity.follows.as_ref().map(|name| name.trim().to_owned()),
            task: identity.task.clone(),
        });
    }
    Ok(identities)
}

/// The shorthand's identity is named after the repository (contract 054 §4.1).
fn single_identity_name(root: &Path) -> String {
    root.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| "release".to_owned())
}

fn resolve_tag_format(configured: Option<&str>) -> Option<String> {
    let trimmed = configured?.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_owned())
}

fn resolve_identity_version_source(
    root: &Path,
    name: &str,
    identity: &ManifestReleaseIdentityConfig,
) -> Result<ResolvedVersionSource, ReleaseError> {
    let Some(configured) = identity.version_file.as_deref().map(str::trim) else {
        return Err(ReleaseError::TaskInvocation(format!(
            "release.identities.{name} is kind `semver` and needs `version-file`"
        )));
    };
    if configured.is_empty() {
        return Err(ReleaseError::TaskInvocation(format!(
            "release.identities.{name}.version-file must not be empty"
        )));
    }
    let path = root.join(configured);
    if !path.exists() {
        return Err(ReleaseError::TaskInvocation(format!(
            "release.identities.{name} version file does not exist: {}",
            path.display()
        )));
    }
    let kind = crate::detect_version_file_kind(&path).ok_or_else(|| {
        ReleaseError::TaskInvocation(format!(
            "release.identities.{name} version file is not a supported kind (expected Cargo.toml, package.json, pyproject.toml or VERSION): {}",
            path.display()
        ))
    })?;
    let field_path =
        crate::resolve_resolved_version_field_path(&path, kind, identity.version_path.as_deref())?;
    Ok(ResolvedVersionSource {
        path,
        kind,
        field_path,
    })
}

pub(crate) fn resolve_gates(
    config: Option<&ManifestReleaseConfig>,
) -> Result<Vec<crate::ResolvedGate>, ReleaseError> {
    let hosted = hosted_evidence_spec_from_manifest(
        config.and_then(|config| config.hosted_evidence.as_ref()),
    )?;
    let Some(config) = config else {
        return Ok(Vec::new());
    };
    let mut gates = Vec::with_capacity(config.gates.len());
    for (name, gate) in &config.gates {
        let (command, task, description, reuse_hosted_evidence, stage) = match gate {
            ManifestReleaseGateConfig::Command(command) => (
                command.trim().to_owned(),
                None,
                None,
                false,
                GateStage::Certify,
            ),
            ManifestReleaseGateConfig::Detailed(ManifestReleaseGateDetails {
                command,
                task,
                description,
                reuse_hosted_evidence,
                stage,
            }) => {
                let command = command.as_deref().map(str::trim).unwrap_or_default();
                let task = task.as_deref().map(str::trim).unwrap_or_default();
                let (command, task) = match (command.is_empty(), task.is_empty()) {
                    (false, true) => (command.to_owned(), None),
                    (true, false) => (format!("effigy {task}"), Some(task.to_owned())),
                    (false, false) => {
                        return Err(ReleaseError::TaskInvocation(format!(
                            "release.gates.{name} sets both `command` and `task`; a gate runs one or the other"
                        )));
                    }
                    (true, true) => {
                        return Err(ReleaseError::TaskInvocation(format!(
                            "release.gates.{name} needs `command` or `task`"
                        )));
                    }
                };
                (
                    command,
                    task,
                    description.clone(),
                    *reuse_hosted_evidence,
                    GateStage::from_manifest(*stage),
                )
            }
        };
        if reuse_hosted_evidence && hosted.is_none() {
            return Err(ReleaseError::TaskInvocation(format!(
                "release gate `{name}` sets reuse-hosted-evidence but [release.hosted-evidence].workflow is not configured"
            )));
        }
        gates.push(crate::ResolvedGate {
            name: name.clone(),
            command,
            task,
            description,
            stage,
            reuse_hosted_evidence,
            hosted_evidence: if reuse_hosted_evidence {
                hosted.clone()
            } else {
                None
            },
        });
    }
    Ok(gates)
}

fn resolve_artifacts(config: Option<&ManifestReleaseConfig>) -> Vec<ResolvedArtifact> {
    let Some(config) = config else {
        return Vec::new();
    };
    config
        .artifacts
        .iter()
        .map(|(name, artifact)| ResolvedArtifact {
            name: name.clone(),
            task: artifact.task.trim().to_owned(),
            path: artifact.path.trim().to_owned(),
        })
        .collect()
}

fn resolve_targets(
    config: Option<&ManifestReleaseConfig>,
) -> Result<Vec<ResolvedTarget>, ReleaseError> {
    let Some(config) = config else {
        return Ok(Vec::new());
    };
    let mut targets = Vec::with_capacity(config.targets.len());
    for (name, target) in &config.targets {
        targets.push(resolve_target(name, target)?);
    }
    Ok(targets)
}

fn resolve_target(
    name: &str,
    target: &ManifestReleaseTargetConfig,
) -> Result<ResolvedTarget, ReleaseError> {
    let task = target
        .task
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let check = target
        .check
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let verify = target
        .verify
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    match target.kind {
        ManifestReleaseTargetKind::Tag => {
            if task.is_some() || check.is_some() {
                return Err(ReleaseError::TaskInvocation(format!(
                    "release.targets.{name} is kind `tag`; remove `task`/`check`, which belong to a `task` target (`verify` may stay)"
                )));
            }
        }
        ManifestReleaseTargetKind::Task => {
            if task.is_none() {
                return Err(ReleaseError::TaskInvocation(format!(
                    "release.targets.{name} is kind `task` and needs `task`: the task that publishes the candidate"
                )));
            }
        }
    }
    Ok(ResolvedTarget {
        name: name.to_owned(),
        kind: match target.kind {
            ManifestReleaseTargetKind::Tag => TargetKind::Tag,
            ManifestReleaseTargetKind::Task => TargetKind::Task,
        },
        task: task.map(str::to_owned),
        check: check.map(str::to_owned),
        verify: verify.map(str::to_owned),
    })
}

fn validate_pipeline_shape(
    config: Option<&ManifestReleaseConfig>,
    identities: &[ResolvedIdentity],
    artifacts: &[ResolvedArtifact],
    targets: &[ResolvedTarget],
) -> Result<(), ReleaseError> {
    if let Some(config) = config {
        for (name, artifact) in &config.artifacts {
            validate_artifact(name, artifact)?;
        }
    }

    let mut seen_source = false;
    for identity in identities {
        match identity.kind {
            IdentityKind::Source => {
                if seen_source {
                    return Err(ReleaseError::TaskInvocation(
                        "a release can declare at most one `source` identity: every identity of that kind is the same source commit".to_owned(),
                    ));
                }
                seen_source = true;
            }
            IdentityKind::Digest => {
                if !artifacts
                    .iter()
                    .any(|artifact| artifact.name == identity.name)
                {
                    return Err(ReleaseError::TaskInvocation(format!(
                        "release.identities.{} is kind `digest` and needs a `[release.artifacts.{}]` entry: the artifact digest is the identity",
                        identity.name, identity.name
                    )));
                }
            }
            IdentityKind::Semver | IdentityKind::Task => {}
        }
        if let Some(follows) = &identity.follows {
            if follows == &identity.name {
                return Err(ReleaseError::TaskInvocation(format!(
                    "release.identities.{} follows itself; `follows` names another semver identity",
                    identity.name
                )));
            }
            let Some(leader) = identities.iter().find(|other| &other.name == follows) else {
                return Err(ReleaseError::TaskInvocation(format!(
                    "release.identities.{} follows `{follows}`, which is not a declared identity",
                    identity.name
                )));
            };
            if leader.kind != IdentityKind::Semver || identity.kind != IdentityKind::Semver {
                return Err(ReleaseError::TaskInvocation(format!(
                    "release.identities.{} follows `{follows}`; `follows` links `semver` identities only",
                    identity.name
                )));
            }
        }
    }

    let has_tag_target = targets.iter().any(|target| target.kind == TargetKind::Tag);
    let has_semver_identity = identities
        .iter()
        .any(|identity| identity.kind == IdentityKind::Semver);
    if has_tag_target && !has_semver_identity {
        return Err(ReleaseError::TaskInvocation(
            "a `tag` target needs a `semver` identity: there is no version to tag".to_owned(),
        ));
    }

    reject_follow_cycles(identities)
}

fn validate_artifact(
    name: &str,
    artifact: &ManifestReleaseArtifactConfig,
) -> Result<(), ReleaseError> {
    if artifact.task.trim().is_empty() {
        return Err(ReleaseError::TaskInvocation(format!(
            "release.artifacts.{name}.task must not be empty"
        )));
    }
    if artifact.path.trim().is_empty() {
        return Err(ReleaseError::TaskInvocation(format!(
            "release.artifacts.{name}.path must not be empty"
        )));
    }
    Ok(())
}

fn reject_follow_cycles(identities: &[ResolvedIdentity]) -> Result<(), ReleaseError> {
    for identity in identities {
        let mut seen = BTreeSet::new();
        let mut cursor = identity;
        while let Some(follows) = cursor.follows.as_deref() {
            if !seen.insert(cursor.name.clone()) {
                return Err(follow_cycle_error(&identity.name));
            }
            match identities.iter().find(|other| other.name == follows) {
                Some(next) => cursor = next,
                None => break,
            }
        }
    }
    Ok(())
}

fn follow_cycle_error(identity: &str) -> ReleaseError {
    ReleaseError::TaskInvocation(format!(
        "release.identities `follows` forms a cycle through `{identity}`; each identity follows at most one leader in a chain"
    ))
}

/// Resolve the whole pipeline, including each identity's next version, without
/// mutating anything.
pub fn plan_release_pipeline(root: &Path) -> Result<ReleasePipelinePlan, ReleaseError> {
    let pipeline = resolve_release_pipeline(root)?;
    let changelog_in_use = has_semver_identity(&pipeline.identities);
    // Changelog bump rules apply only to `semver` identities (contract 054
    // §4.3), so a release with none neither reads nor is blocked by a
    // changelog: the file may be absent or unparseable.
    let (parsed_changelog, changelog_valid, changelog_diagnostics) = if changelog_in_use {
        let raw_changelog = std::fs::read_to_string(&pipeline.changelog_path).map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "failed to read changelog {}: {error}",
                pipeline.changelog_path.display()
            ))
        })?;
        let parsed = changelog::parse(&raw_changelog)
            .map_err(|error| ReleaseError::TaskInvocation(error.to_string()))?;
        let diagnostics = changelog::validate(&parsed, &raw_changelog)
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let valid = diagnostics.is_empty();
        (Some(parsed), valid, diagnostics)
    } else {
        (None, true, Vec::new())
    };
    let counts = parsed_changelog
        .as_ref()
        .map(unreleased_counts)
        .unwrap_or_default();
    let unreleased_empty = counts.values().copied().sum::<usize>() == 0;
    let initial_tag_current_version = pipeline.initial_tag_current_version
        && parsed_changelog
            .as_ref()
            .is_some_and(|parsed| parsed.latest_version().is_none());

    let mut blockers = Vec::new();
    if changelog_in_use {
        if !changelog_diagnostics.is_empty() {
            blockers.push(format!(
                "changelog validation failed with {} issue(s)",
                changelog_diagnostics.len()
            ));
        }
        if unreleased_empty {
            blockers.push("unreleased changelog section has no entries".to_owned());
        }
    }

    let mut identities: Vec<ReleaseIdentityPlan> = Vec::with_capacity(pipeline.identities.len());
    for identity in &pipeline.identities {
        identities.push(resolve_identity_plan(
            &pipeline,
            identity,
            parsed_changelog.as_ref(),
            initial_tag_current_version,
            &mut blockers,
        )?);
    }
    bind_followers(&mut identities, &mut blockers);

    let latest_changelog_version = parsed_changelog
        .as_ref()
        .and_then(|parsed| parsed.latest_version())
        .and_then(|release| release.version.clone());
    if let Some(latest) = latest_changelog_version {
        for identity in identities
            .iter()
            .filter(|identity| identity.kind == IdentityKind::Semver)
        {
            if let Some(current) = &identity.current_version {
                if latest != *current {
                    blockers.push(format!(
                        "version file reports {current} but latest changelog release is {latest}"
                    ));
                }
            }
        }
    }

    for identity in identities
        .iter()
        .filter(|identity| identity.kind == IdentityKind::Semver)
    {
        if let Some(tag) = &identity.tag {
            if git_tag_exists(root, tag)? {
                blockers.push(format!("release tag already exists locally: {tag}"));
            }
        }
    }

    let artifacts = pipeline
        .artifacts
        .iter()
        .map(|artifact| ReleaseArtifactPlan {
            name: artifact.name.clone(),
            task: artifact.task.clone(),
            path: artifact.path.clone(),
            identity_kind: pipeline
                .identity(&artifact.name)
                .map(|identity| identity.kind.label().to_owned()),
        })
        .collect::<Vec<_>>();

    let mut files = Vec::new();
    let mut seen_files = BTreeSet::new();
    for identity in &identities {
        for file in &identity.files {
            if seen_files.insert(file.clone()) {
                files.push(file.clone());
            }
        }
    }
    if changelog_in_use && seen_files.insert(pipeline.changelog_path.clone()) {
        files.push(pipeline.changelog_path.clone());
    }
    for sync in &pipeline.sync_files {
        if seen_files.insert(sync.path.clone()) {
            files.push(sync.path.clone());
        }
    }

    let working_tree_clean = git_work_tree_clean(root)?;

    Ok(ReleasePipelinePlan {
        repo_root: root.to_path_buf(),
        single_identity_shorthand: pipeline.single_identity_shorthand,
        changelog_path: pipeline.changelog_path.clone(),
        changelog_in_use,
        changelog_valid,
        changelog_diagnostics,
        unreleased_counts: counts,
        unreleased_empty,
        existing_changelog_versions: parsed_changelog
            .as_ref()
            .map(|parsed| {
                parsed
                    .releases
                    .iter()
                    .filter_map(|release| release.version.as_ref().map(ToString::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        identities,
        gates: pipeline.gates.clone(),
        artifacts,
        targets: pipeline.targets.clone(),
        approvals: pipeline.approvals.clone(),
        approval_defaulted: pipeline.approval_defaulted,
        files,
        working_tree_clean,
        ready: blockers.is_empty(),
        blockers,
    })
}

fn resolve_identity_plan(
    pipeline: &ReleasePipeline,
    identity: &ResolvedIdentity,
    parsed_changelog: Option<&changelog::Changelog>,
    initial_tag_current_version: bool,
    blockers: &mut Vec<String>,
) -> Result<ReleaseIdentityPlan, ReleaseError> {
    match identity.kind {
        IdentityKind::Semver => {
            let source = identity
                .version_source
                .as_ref()
                .expect("semver identity resolved a version source");
            let current_version = read_current_version(source)?;
            let bump = if initial_tag_current_version {
                BumpKind::None
            } else {
                parsed_changelog
                    .map(|parsed| suggested_bump(parsed, &current_version, pipeline.pre_1_0))
                    .unwrap_or(BumpKind::None)
            };
            let next_version = if initial_tag_current_version {
                Some(current_version.clone())
            } else {
                apply_bump(&current_version, bump)
            };
            let tag = next_version.as_ref().map(|version| {
                format_release_tag(
                    identity.tag_format.as_deref().unwrap_or("v{version}"),
                    version,
                )
            });
            let files = if next_version.is_some() {
                vec![source.path.clone()]
            } else {
                Vec::new()
            };
            Ok(ReleaseIdentityPlan {
                name: identity.name.clone(),
                kind: identity.kind,
                current_version: Some(current_version),
                next_version,
                identity_value: None,
                bump: Some(bump.to_string()),
                tag,
                follows: identity.follows.clone(),
                version_source: identity.version_source.clone(),
                tag_format: identity.tag_format.clone(),
                task: None,
                files,
            })
        }
        IdentityKind::Source => {
            let sha = match git_head_sha(&pipeline.repo_root) {
                Ok(sha) => Some(sha),
                Err(error) => {
                    blockers.push(format!(
                        "identity `{}` is kind `source` and the source commit could not be read: {error}",
                        identity.name
                    ));
                    None
                }
            };
            Ok(release_plan_without_version(identity, sha))
        }
        IdentityKind::Digest => Ok(release_plan_without_version(identity, None)),
        IdentityKind::Task => Ok(release_plan_without_version(identity, None)),
    }
}

fn release_plan_without_version(
    identity: &ResolvedIdentity,
    identity_value: Option<String>,
) -> ReleaseIdentityPlan {
    ReleaseIdentityPlan {
        name: identity.name.clone(),
        kind: identity.kind,
        current_version: None,
        next_version: None,
        identity_value,
        bump: None,
        tag: None,
        follows: None,
        version_source: None,
        tag_format: None,
        task: identity.task.clone(),
        files: Vec::new(),
    }
}

/// A follower takes its leader's resolved version, so the pair moves together.
///
/// Contract 054 §4.2 says `follows` means "bump together with core; omit for
/// independent bumps". One changelog feeds every identity, so the version
/// number is what "together" has to mean: the follower is released at the
/// leader's next version, under its own tag format. 054 records the choice.
fn bind_followers(identities: &mut [ReleaseIdentityPlan], blockers: &mut Vec<String>) {
    let mut leader_for: BTreeMap<String, String> = BTreeMap::new();
    for identity in identities.iter() {
        if let Some(follows) = identity.follows.clone() {
            leader_for.insert(identity.name.clone(), follows);
        }
    }

    // Resolve each follower against its leader, walking the chain so a follower
    // of a follower binds to the same version.
    let mut leaders: BTreeMap<String, String> = BTreeMap::new();
    for follower in leader_for.keys() {
        let mut cursor = follower.clone();
        let mut seen = BTreeSet::new();
        while let Some(next) = leader_for.get(&cursor) {
            if !seen.insert(cursor.clone()) {
                break;
            }
            cursor = next.clone();
        }
        leaders.insert(follower.clone(), cursor);
    }

    let leader_versions: BTreeMap<String, (Option<semver::Version>, Option<String>)> = identities
        .iter()
        .map(|identity| {
            (
                identity.name.clone(),
                (identity.next_version.clone(), identity.bump.clone()),
            )
        })
        .collect();

    for identity in identities.iter_mut() {
        let Some(leader) = leaders.get(&identity.name) else {
            continue;
        };
        let Some((leader_version, leader_bump)) = leader_versions.get(leader) else {
            continue;
        };
        if identity.kind != IdentityKind::Semver {
            continue;
        }
        let Some(version) = leader_version.clone() else {
            identity.next_version = None;
            identity.tag = None;
            identity.files.clear();
            continue;
        };
        if let Some(current) = &identity.current_version {
            if version <= *current {
                blockers.push(format!(
                    "identity `{}` follows `{leader}`, whose next version {version} is not greater than its current {current}",
                    identity.name
                ));
            }
        }
        identity.tag = identity
            .tag_format
            .as_deref()
            .map(|format| format_release_tag(format, &version));
        identity.next_version = Some(version);
        identity.bump = leader_bump.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_stages_cover_the_contracts_stage_boundaries() {
        assert_eq!(RELEASE_STAGES[0], "validate");
        assert_eq!(RELEASE_STAGES[6], "closeout");
        assert!(RELEASE_STAGES.contains(&DEFAULT_APPROVAL_CHECKPOINT));
    }

    #[test]
    fn identity_kind_labels_are_the_config_values() {
        assert_eq!(IdentityKind::Semver.label(), "semver");
        assert_eq!(IdentityKind::Digest.label(), "digest");
        assert_eq!(IdentityKind::Source.label(), "source");
        assert_eq!(IdentityKind::Task.label(), "task");
    }
}
