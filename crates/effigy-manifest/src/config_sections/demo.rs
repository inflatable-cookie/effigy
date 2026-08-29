use std::path::Path;

use crate::ManifestError;
use crate::ManifestManagedRun;

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
