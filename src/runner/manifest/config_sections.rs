use std::collections::BTreeMap;
use std::path::Path;

use crate::runner::error::RunnerError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(in crate::runner) enum ManifestScanOutputFormat {
    Text,
    Markdown,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestShellConfig {
    #[serde(default)]
    pub(in crate::runner) run: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestPackageManagerConfig {
    #[serde(default, alias = "js_ts", alias = "typescript")]
    pub(in crate::runner) js: Option<ManifestJsPackageManager>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestScanConfig {
    #[serde(default)]
    pub(in crate::runner) god_files: Option<ManifestGodFilesConfig>,
    #[serde(default)]
    pub(in crate::runner) duplicate_blocks: Option<ManifestDuplicateBlocksConfig>,
    #[serde(default)]
    pub(in crate::runner) comment_ratio: Option<ManifestCommentRatioConfig>,
    #[serde(default)]
    pub(in crate::runner) generated_assets: Option<ManifestGeneratedAssetsConfig>,
    #[serde(default)]
    pub(in crate::runner) generated_in_src: Option<ManifestGeneratedInSrcConfig>,
    #[serde(default)]
    pub(in crate::runner) attention_markers: Option<ManifestAttentionMarkersConfig>,
    #[serde(default)]
    pub(in crate::runner) stale_suppressions: Option<ManifestStaleSuppressionsConfig>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestGodFilesConfig {
    #[serde(default, alias = "threshold")]
    pub(in crate::runner) warn: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) high: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) critical: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) fail_on_findings: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) respect_gitignore: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) doctor: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) include: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) exclude: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub(in crate::runner) out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestDuplicateBlocksConfig {
    #[serde(default, alias = "threshold")]
    pub(in crate::runner) warn: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) high: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) critical: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) min_occurrences: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) fail_on_findings: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) respect_gitignore: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) doctor: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) include: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) exclude: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub(in crate::runner) out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestCommentRatioConfig {
    #[serde(default, alias = "threshold")]
    pub(in crate::runner) warn: Option<f64>,
    #[serde(default)]
    pub(in crate::runner) high: Option<f64>,
    #[serde(default)]
    pub(in crate::runner) critical: Option<f64>,
    #[serde(default)]
    pub(in crate::runner) min_code_lines: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) fail_on_findings: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) respect_gitignore: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) doctor: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) include: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) exclude: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub(in crate::runner) out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestGeneratedAssetsConfig {
    #[serde(default, alias = "threshold")]
    pub(in crate::runner) warn: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) high: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) critical: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) fail_on_findings: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) respect_gitignore: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) doctor: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) include: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) exclude: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub(in crate::runner) out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestGeneratedInSrcConfig {
    #[serde(default, alias = "threshold")]
    pub(in crate::runner) warn: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) warn_bytes: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) high: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) high_bytes: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) critical: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) critical_bytes: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) source_root: Option<String>,
    #[serde(default)]
    pub(in crate::runner) source_roots: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) fail_on_findings: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) respect_gitignore: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) doctor: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) include: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) exclude: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub(in crate::runner) out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestAttentionMarkersConfig {
    #[serde(default)]
    pub(in crate::runner) warning: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) high: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) critical: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) fail_on_findings: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) respect_gitignore: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) doctor: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) include: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) exclude: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub(in crate::runner) out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestStaleSuppressionsConfig {
    #[serde(default)]
    pub(in crate::runner) warning: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) high: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) critical: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) fail_on_findings: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) respect_gitignore: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) doctor: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) include: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) exclude: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub(in crate::runner) out: Option<String>,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(in crate::runner) enum ManifestJsPackageManager {
    Bun,
    Pnpm,
    Npm,
    Direct,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestEnvSchemaConfig {
    #[serde(default)]
    pub(in crate::runner) enabled: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) schema: Option<String>,
    #[serde(default)]
    pub(in crate::runner) exec_timeout: Option<u64>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestDocsPolicyConfig {
    #[serde(default)]
    pub(in crate::runner) indexes: BTreeMap<String, ManifestDocsPolicyIndexConfig>,
    #[serde(default, alias = "next_actions")]
    pub(in crate::runner) next_actions: BTreeMap<String, ManifestDocsPolicyNextActionConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(in crate::runner) enum ManifestDemoMode {
    Headless,
    Interactive,
    Hybrid,
}

impl ManifestDemoMode {
    pub(in crate::runner) fn as_str(self) -> &'static str {
        match self {
            Self::Headless => "headless",
            Self::Interactive => "interactive",
            Self::Hybrid => "hybrid",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(in crate::runner) enum ManifestDemoStatus {
    Planned,
    Ready,
    Running,
    Passed,
    Failed,
    Broken,
    Missing,
}

impl ManifestDemoStatus {
    pub(in crate::runner) fn as_str(self) -> &'static str {
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
pub(in crate::runner) struct ManifestDemoConfig {
    pub(in crate::runner) title: String,
    pub(in crate::runner) summary: String,
    pub(in crate::runner) proof: String,
    pub(in crate::runner) owner: String,
    pub(in crate::runner) mode: ManifestDemoMode,
    pub(in crate::runner) status: ManifestDemoStatus,
    pub(in crate::runner) covers: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) tags: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) artifacts: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) receipt: Option<String>,
    #[serde(default)]
    pub(in crate::runner) task: Option<String>,
    #[serde(default)]
    pub(in crate::runner) run: Option<String>,
    #[serde(default)]
    pub(in crate::runner) prerequisites: Vec<String>,
    #[serde(default, alias = "depends_on")]
    pub(in crate::runner) dependencies: Vec<String>,
}

impl ManifestDemoConfig {
    pub(in crate::runner) fn validate(
        &self,
        manifest_path: &Path,
        demo_id: &str,
    ) -> Result<(), RunnerError> {
        validate_non_empty_demo_string(manifest_path, demo_id, "title", &self.title)?;
        validate_non_empty_demo_string(manifest_path, demo_id, "summary", &self.summary)?;
        validate_non_empty_demo_string(manifest_path, demo_id, "proof", &self.proof)?;
        validate_non_empty_demo_string(manifest_path, demo_id, "owner", &self.owner)?;

        if self.covers.is_empty() {
            return Err(RunnerError::TaskManifestCompose {
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
                validate_non_empty_demo_string(manifest_path, demo_id, "run", run)?;
            }
            (Some(_), Some(_)) | (None, None) => {
                return Err(RunnerError::TaskManifestCompose {
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

fn validate_non_empty_demo_string(
    manifest_path: &Path,
    demo_id: &str,
    field: &str,
    value: &str,
) -> Result<(), RunnerError> {
    if value.trim().is_empty() {
        return Err(RunnerError::TaskManifestCompose {
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
) -> Result<(), RunnerError> {
    for (index, value) in values.iter().enumerate() {
        if value.trim().is_empty() {
            return Err(RunnerError::TaskManifestCompose {
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
pub(in crate::runner) struct ManifestDocsPolicyIndexConfig {
    pub(in crate::runner) file: String,
    pub(in crate::runner) dir: String,
    #[serde(default)]
    pub(in crate::runner) section: Option<String>,
    #[serde(default)]
    pub(in crate::runner) exclude: Vec<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestDocsPolicyNextActionConfig {
    pub(in crate::runner) index: String,
    pub(in crate::runner) heading: String,
    #[serde(alias = "allowlist_file")]
    pub(in crate::runner) allowlist_file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestBootstrapConfig {
    #[serde(default)]
    pub(in crate::runner) setup: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) start: Option<String>,
    #[serde(default)]
    pub(in crate::runner) submodules: Option<ManifestBootstrapSubmodulesPolicy>,
    #[serde(default)]
    pub(in crate::runner) children: Vec<ManifestBootstrapChildConfig>,
}

#[derive(Debug, serde::Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(in crate::runner) enum ManifestBootstrapSubmodulesPolicy {
    None,
    Init,
    Recursive,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestBootstrapChildConfig {
    pub(in crate::runner) path: String,
    pub(in crate::runner) repo: String,
    #[serde(default)]
    pub(in crate::runner) branch: Option<String>,
    #[serde(default)]
    pub(in crate::runner) setup: Vec<String>,
    #[serde(default = "default_bootstrap_child_required")]
    pub(in crate::runner) required: bool,
}

fn default_bootstrap_child_required() -> bool {
    true
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestReleaseConfig {
    #[serde(default)]
    pub(in crate::runner) version_file: Option<String>,
    #[serde(default)]
    pub(in crate::runner) version_path: Option<String>,
    #[serde(default)]
    pub(in crate::runner) changelog: Option<String>,
    #[serde(default, rename = "pre-1-0")]
    pub(in crate::runner) pre_1_0: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) sync_files: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) gates: BTreeMap<String, ManifestReleaseGateConfig>,
    #[serde(default)]
    pub(in crate::runner) tag_format: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub(in crate::runner) enum ManifestReleaseGateConfig {
    Command(String),
    Detailed(ManifestReleaseGateDetails),
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestReleaseGateDetails {
    pub(in crate::runner) command: String,
    #[serde(default)]
    pub(in crate::runner) description: Option<String>,
}
