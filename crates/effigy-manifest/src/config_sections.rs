use std::collections::BTreeMap;
use std::path::Path;

use crate::ManifestError;
use crate::ManifestManagedRun;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestScanOutputFormat {
    Text,
    Markdown,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestShellConfig {
    #[serde(default)]
    pub run: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestPackageManagerConfig {
    #[serde(default, alias = "js_ts", alias = "typescript")]
    pub js: Option<ManifestJsPackageManager>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestScanConfig {
    #[serde(default)]
    pub god_files: Option<ManifestGodFilesConfig>,
    #[serde(default)]
    pub duplicate_blocks: Option<ManifestDuplicateBlocksConfig>,
    #[serde(default)]
    pub comment_ratio: Option<ManifestCommentRatioConfig>,
    #[serde(default)]
    pub generated_assets: Option<ManifestGeneratedAssetsConfig>,
    #[serde(default)]
    pub generated_in_src: Option<ManifestGeneratedInSrcConfig>,
    #[serde(default)]
    pub attention_markers: Option<ManifestAttentionMarkersConfig>,
    #[serde(default)]
    pub stale_suppressions: Option<ManifestStaleSuppressionsConfig>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestGodFilesConfig {
    #[serde(default, alias = "threshold")]
    pub warn: Option<usize>,
    #[serde(default)]
    pub high: Option<usize>,
    #[serde(default)]
    pub critical: Option<usize>,
    #[serde(default)]
    pub fail_on_findings: Option<bool>,
    #[serde(default)]
    pub respect_gitignore: Option<bool>,
    #[serde(default)]
    pub doctor: Option<bool>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestDuplicateBlocksConfig {
    #[serde(default, alias = "threshold")]
    pub warn: Option<usize>,
    #[serde(default)]
    pub high: Option<usize>,
    #[serde(default)]
    pub critical: Option<usize>,
    #[serde(default)]
    pub min_occurrences: Option<usize>,
    #[serde(default)]
    pub fail_on_findings: Option<bool>,
    #[serde(default)]
    pub respect_gitignore: Option<bool>,
    #[serde(default)]
    pub doctor: Option<bool>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestCommentRatioConfig {
    #[serde(default, alias = "threshold")]
    pub warn: Option<f64>,
    #[serde(default)]
    pub high: Option<f64>,
    #[serde(default)]
    pub critical: Option<f64>,
    #[serde(default)]
    pub min_code_lines: Option<usize>,
    #[serde(default)]
    pub fail_on_findings: Option<bool>,
    #[serde(default)]
    pub respect_gitignore: Option<bool>,
    #[serde(default)]
    pub doctor: Option<bool>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestGeneratedAssetsConfig {
    #[serde(default, alias = "threshold")]
    pub warn: Option<usize>,
    #[serde(default)]
    pub high: Option<usize>,
    #[serde(default)]
    pub critical: Option<usize>,
    #[serde(default)]
    pub fail_on_findings: Option<bool>,
    #[serde(default)]
    pub respect_gitignore: Option<bool>,
    #[serde(default)]
    pub doctor: Option<bool>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestGeneratedInSrcConfig {
    #[serde(default, alias = "threshold")]
    pub warn: Option<usize>,
    #[serde(default)]
    pub warn_bytes: Option<usize>,
    #[serde(default)]
    pub high: Option<usize>,
    #[serde(default)]
    pub high_bytes: Option<usize>,
    #[serde(default)]
    pub critical: Option<usize>,
    #[serde(default)]
    pub critical_bytes: Option<usize>,
    #[serde(default)]
    pub source_root: Option<String>,
    #[serde(default)]
    pub source_roots: Vec<String>,
    #[serde(default)]
    pub fail_on_findings: Option<bool>,
    #[serde(default)]
    pub respect_gitignore: Option<bool>,
    #[serde(default)]
    pub doctor: Option<bool>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestAttentionMarkersConfig {
    #[serde(default)]
    pub warning: Vec<String>,
    #[serde(default)]
    pub high: Vec<String>,
    #[serde(default)]
    pub critical: Vec<String>,
    #[serde(default)]
    pub fail_on_findings: Option<bool>,
    #[serde(default)]
    pub respect_gitignore: Option<bool>,
    #[serde(default)]
    pub doctor: Option<bool>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestStaleSuppressionsConfig {
    #[serde(default)]
    pub warning: Vec<String>,
    #[serde(default)]
    pub high: Vec<String>,
    #[serde(default)]
    pub critical: Vec<String>,
    #[serde(default)]
    pub fail_on_findings: Option<bool>,
    #[serde(default)]
    pub respect_gitignore: Option<bool>,
    #[serde(default)]
    pub doctor: Option<bool>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ManifestJsPackageManager {
    Bun,
    Pnpm,
    Npm,
    Direct,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestEnvSchemaConfig {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub exec_timeout: Option<u64>,
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct ManifestContainersConfig {
    #[serde(default)]
    pub default: Option<String>,
    #[serde(flatten)]
    pub environments: BTreeMap<String, ManifestContainerConfig>,
}

#[derive(Debug, serde::Deserialize)]
pub struct ManifestContainerConfig {
    #[serde(default)]
    pub driver: Option<ManifestContainerDriver>,
    #[serde(default)]
    pub startup: Option<ManifestContainerStartup>,
    #[serde(default)]
    pub profile: Option<String>,
    pub compose_file: String,
    #[serde(default)]
    pub project_name: Option<String>,
    #[serde(default)]
    pub primary_service: Option<String>,
    #[serde(default)]
    pub lifecycle: Option<ManifestContainerLifecycleConfig>,
    #[serde(default)]
    pub health: Option<ManifestContainerHealthConfig>,
    #[serde(default)]
    pub host: Option<ManifestContainerHostConfig>,
    #[serde(default)]
    pub ui: Option<ManifestContainerUiConfig>,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ManifestContainerDriver {
    Colima,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ManifestContainerStartup {
    Attached,
    Detached,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestContainerOnTaskExit {
    Stop,
    LeaveRunning,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ManifestContainerShutdownMode {
    Graceful,
    Immediate,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestContainerLifecycleConfig {
    #[serde(default)]
    pub on_task_exit: Option<ManifestContainerOnTaskExit>,
    #[serde(default)]
    pub shutdown: Option<ManifestContainerShutdownMode>,
    #[serde(default)]
    pub detach_timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestContainerHealthConfig {
    #[serde(default)]
    pub check: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestContainerHostConfig {
    #[serde(default)]
    pub ports: Vec<String>,
    #[serde(default)]
    pub mounts: Vec<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestContainerUiConfig {
    #[serde(default)]
    pub tabs: Vec<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDocsPolicyConfig {
    #[serde(default)]
    pub indexes: BTreeMap<String, ManifestDocsPolicyIndexConfig>,
    #[serde(default, alias = "next_actions")]
    pub next_actions: BTreeMap<String, ManifestDocsPolicyNextActionConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ManifestDemoMode {
    Headless,
    Interactive,
    Hybrid,
}

impl ManifestDemoMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Headless => "headless",
            Self::Interactive => "interactive",
            Self::Hybrid => "hybrid",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ManifestDemoStatus {
    Planned,
    Ready,
    Running,
    Passed,
    Failed,
    Broken,
    Missing,
}

impl ManifestDemoStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Broken => "broken",
            Self::Missing => "missing",
        }
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestDemoConfig {
    pub title: String,
    pub summary: String,
    pub proof: String,
    pub owner: String,
    pub mode: ManifestDemoMode,
    pub status: ManifestDemoStatus,
    pub covers: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub artifacts: Vec<String>,
    #[serde(default)]
    pub receipt: Option<String>,
    #[serde(default)]
    pub task: Option<String>,
    #[serde(default)]
    pub run: Option<ManifestManagedRun>,
    #[serde(default)]
    pub prerequisites: Vec<String>,
    #[serde(default, alias = "depends_on")]
    pub dependencies: Vec<String>,
}

impl ManifestDemoConfig {
    pub fn validate(&self, manifest_path: &Path, demo_id: &str) -> Result<(), ManifestError> {
        validate_non_empty_demo_string(manifest_path, demo_id, "title", &self.title)?;
        validate_non_empty_demo_string(manifest_path, demo_id, "summary", &self.summary)?;
        validate_non_empty_demo_string(manifest_path, demo_id, "proof", &self.proof)?;
        validate_non_empty_demo_string(manifest_path, demo_id, "owner", &self.owner)?;

        if self.covers.is_empty() {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!(
                    "invalid `[demos.{demo_id}]`: `covers` must contain at least one coverage key"
                ),
            });
        }

        validate_demo_string_list(manifest_path, demo_id, "covers", &self.covers)?;
        validate_demo_string_list(manifest_path, demo_id, "tags", &self.tags)?;
        validate_demo_string_list(manifest_path, demo_id, "artifacts", &self.artifacts)?;
        validate_demo_string_list(manifest_path, demo_id, "prerequisites", &self.prerequisites)?;
        validate_demo_string_list(manifest_path, demo_id, "dependencies", &self.dependencies)?;

        if let Some(receipt) = &self.receipt {
            validate_non_empty_demo_string(manifest_path, demo_id, "receipt", receipt)?;
        }

        match (&self.task, &self.run) {
            (Some(task), None) => {
                validate_non_empty_demo_string(manifest_path, demo_id, "task", task)?;
            }
            (None, Some(run)) => {
                validate_demo_run(manifest_path, demo_id, run)?;
            }
            (Some(_), Some(_)) | (None, None) => {
                return Err(ManifestError::Compose {
                    path: manifest_path.to_path_buf(),
                    detail: format!(
                        "invalid `[demos.{demo_id}]`: declare exactly one runnable entrypoint via `task` or `run`"
                    ),
                });
            }
        }

        Ok(())
    }
}

fn validate_demo_run(
    manifest_path: &Path,
    demo_id: &str,
    run: &ManifestManagedRun,
) -> Result<(), ManifestError> {
    match run {
        ManifestManagedRun::Command(command) => {
            validate_non_empty_demo_string(manifest_path, demo_id, "run", command)
        }
        ManifestManagedRun::Sequence(steps) => {
            if steps.is_empty() {
                return Err(ManifestError::Compose {
                    path: manifest_path.to_path_buf(),
                    detail: format!(
                        "invalid `[demos.{demo_id}]`: `run` sequence must contain at least one step"
                    ),
                });
            }
            Ok(())
        }
    }
}

fn validate_non_empty_demo_string(
    manifest_path: &Path,
    demo_id: &str,
    field: &str,
    value: &str,
) -> Result<(), ManifestError> {
    if value.trim().is_empty() {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("invalid `[demos.{demo_id}]`: `{field}` must be a non-empty string"),
        });
    }
    Ok(())
}

fn validate_demo_string_list(
    manifest_path: &Path,
    demo_id: &str,
    field: &str,
    values: &[String],
) -> Result<(), ManifestError> {
    for (index, value) in values.iter().enumerate() {
        if value.trim().is_empty() {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!(
                    "invalid `[demos.{demo_id}]`: `{field}[{index}]` must be a non-empty string"
                ),
            });
        }
    }
    Ok(())
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDocsPolicyIndexConfig {
    pub file: String,
    pub dir: String,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDocsPolicyNextActionConfig {
    pub index: String,
    pub heading: String,
    #[serde(alias = "allowlist_file")]
    pub allowlist_file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestBootstrapConfig {
    #[serde(default)]
    pub setup: Vec<String>,
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default)]
    pub submodules: Option<ManifestBootstrapSubmodulesPolicy>,
    #[serde(default)]
    pub children: Vec<ManifestBootstrapChildConfig>,
}

#[derive(Debug, serde::Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestBootstrapSubmodulesPolicy {
    None,
    Init,
    Recursive,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestBootstrapChildConfig {
    pub path: String,
    pub repo: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub setup: Vec<String>,
    #[serde(default = "default_bootstrap_child_required")]
    pub required: bool,
}

fn default_bootstrap_child_required() -> bool {
    true
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDistributionConfig {
    #[serde(default)]
    pub package: Option<ManifestDistributionPackageConfig>,
    #[serde(default)]
    pub publish: Option<ManifestDistributionPublishConfig>,
    #[serde(default)]
    pub preflight: Option<ManifestDistributionPreflightConfig>,
    #[serde(default)]
    pub metadata: Option<ManifestDistributionMetadataConfig>,
    #[serde(default)]
    pub closeout: Option<ManifestDistributionCloseoutConfig>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDistributionPackageConfig {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub repo_url: Option<String>,
    #[serde(default)]
    pub brew_formula: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDistributionPublishConfig {
    #[serde(default)]
    pub binary_name: Option<String>,
    #[serde(default)]
    pub registry_label: Option<String>,
    #[serde(default)]
    pub verify_tag_install: Option<bool>,
    #[serde(default)]
    pub verify_binary_json_tasks: Option<bool>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDistributionPreflightConfig {
    #[serde(default)]
    pub docs_task: Option<String>,
    #[serde(default)]
    pub smoke_task: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDistributionMetadataConfig {
    #[serde(default)]
    pub required_docs: Option<Vec<String>>,
    #[serde(default)]
    pub required_files: Option<Vec<String>>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDistributionCloseoutConfig {
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub related: Option<String>,
    #[serde(default)]
    pub next_step: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestReleaseConfig {
    #[serde(default)]
    pub version_file: Option<String>,
    #[serde(default)]
    pub version_path: Option<String>,
    #[serde(default)]
    pub changelog: Option<String>,
    #[serde(default, rename = "pre-1-0")]
    pub pre_1_0: Option<bool>,
    #[serde(default)]
    pub sync_files: Vec<String>,
    #[serde(default)]
    pub gates: BTreeMap<String, ManifestReleaseGateConfig>,
    #[serde(default)]
    pub tag_format: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub enum ManifestReleaseGateConfig {
    Command(String),
    Detailed(ManifestReleaseGateDetails),
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestReleaseGateDetails {
    pub command: String,
    #[serde(default)]
    pub description: Option<String>,
}
