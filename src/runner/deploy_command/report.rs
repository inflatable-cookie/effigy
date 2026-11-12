use std::fs;
use std::path::{Path, PathBuf};

use effigy_state::{path_display, safe_path_component};
use serde::Serialize;
use serde_json::{to_value, Value};

use crate::runner::error::RunnerError;

use super::provider::DeployProviderProvenance;

pub(super) const PLAN_SCHEMA: &str = "effigy.deploy.plan.v1";
pub(super) const APPLY_SCHEMA: &str = "effigy.deploy.apply.v1";
pub(super) const STATUS_SCHEMA: &str = "effigy.deploy.status.v1";
pub(super) const HISTORY_SCHEMA: &str = "effigy.deploy.history.v1";

#[derive(Debug, Clone, Serialize)]
pub(super) struct DeployPlanReport {
    pub(super) schema: String,
    pub(super) schema_version: u8,
    pub(super) deployment_id: String,
    pub(super) env: String,
    pub(super) provider: String,
    /// The resolved provider package commit and how it was obtained.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) provider_provenance: Option<DeployProviderProvenance>,
    pub(super) app: DeployPlanApp,
    pub(super) code: DeployCodeRef,
    pub(super) release_policy: DeployReleasePolicyReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) state: Option<DeployStatePlan>,
    pub(super) artifact_policy: DeployArtifactPolicyReport,
    pub(super) provider_preflight: DeployProviderPreflightReport,
    pub(super) hooks: Vec<DeployHookPlan>,
    pub(super) health_checks: Vec<DeployHealthPlan>,
    pub(super) warnings: Vec<String>,
    pub(super) blockers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) written_report_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) written_history_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct DeployPlanApp {
    pub(super) name: String,
    pub(super) project_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct DeployCodeRef {
    pub(super) requested_ref: String,
    pub(super) kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) resolved_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) resolved_commit: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct DeployReleasePolicyReport {
    pub(super) mode: String,
    pub(super) required: bool,
    pub(super) gates_required: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct DeployStatePlan {
    pub(super) stack: String,
    pub(super) lineage_id: String,
    pub(super) planned_report_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct DeployArtifactPolicyReport {
    pub(super) mode: String,
    pub(super) blockers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct DeployProviderPreflightReport {
    pub(super) status: String,
    pub(super) checks: Vec<DeployProviderCheck>,
    pub(super) blockers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct DeployProviderCheck {
    pub(super) name: String,
    pub(super) status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct DeployHookPlan {
    pub(super) stage: String,
    pub(super) task: String,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct DeployHealthPlan {
    pub(super) service: String,
    pub(super) kind: String,
    pub(super) path: String,
}

#[derive(Debug, Serialize)]
pub(super) struct DeployStatusReport {
    pub(super) schema: String,
    pub(super) schema_version: u8,
    pub(super) env: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) active_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) latest_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) active: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) latest: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) provider_status: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) provider_provenance: Option<DeployProviderProvenance>,
    pub(super) warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct DeployHistoryReport {
    pub(super) schema: String,
    pub(super) schema_version: u8,
    pub(super) env: String,
    pub(super) entries: Vec<DeployHistoryItem>,
    pub(super) warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct DeployHistoryItem {
    pub(super) path: String,
    pub(super) deployment_id: String,
    pub(super) schema: String,
    pub(super) status: String,
}

pub(super) struct DeployReportPaths {
    pub(super) latest_path: PathBuf,
    pub(super) history_path: PathBuf,
}

pub(super) fn deploy_active_path(repo_root: &Path, env: &str) -> PathBuf {
    repo_root
        .join(".effigy")
        .join("runtime")
        .join("deploy")
        .join("active")
        .join(format!("{}.json", safe_path_component(env)))
}

pub(super) fn deploy_latest_path(repo_root: &Path, env: &str) -> PathBuf {
    repo_root
        .join(".effigy")
        .join("reports")
        .join("deploy")
        .join(safe_path_component(env))
        .join("latest.json")
}

pub(super) fn deploy_history_dir(repo_root: &Path, env: &str) -> PathBuf {
    repo_root
        .join(".effigy")
        .join("reports")
        .join("deploy")
        .join(safe_path_component(env))
        .join("history")
}

pub(super) fn deploy_report_paths(
    repo_root: &Path,
    env: &str,
    deployment_id: &str,
) -> DeployReportPaths {
    DeployReportPaths {
        latest_path: deploy_latest_path(repo_root, env),
        history_path: deploy_history_dir(repo_root, env)
            .join(format!("{}.json", safe_path_component(deployment_id))),
    }
}

pub(super) fn write_json_report<T: Serialize>(
    repo_root: &Path,
    paths: &[&Path],
    report: &T,
) -> Result<(), RunnerError> {
    let encoded = serde_json::to_string_pretty(report)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    for path in paths {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                RunnerError::task_invocation(format!(
                    "failed to create deploy report directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        fs::write(path, format!("{encoded}\n")).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to write deploy report {}: {error}",
                path_display(path, repo_root)
            ))
        })?;
    }
    Ok(())
}

pub(super) fn read_optional_json(path: &Path) -> Result<Option<Value>, RunnerError> {
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path).map_err(|error| {
        RunnerError::task_invocation(format!("failed to read {}: {error}", path.display()))
    })?;
    let value = serde_json::from_str(&text).map_err(|error| {
        RunnerError::task_invocation(format!("failed to parse {}: {error}", path.display()))
    })?;
    Ok(Some(value))
}

pub(super) fn json_value<T: Serialize>(value: &T) -> Result<Value, RunnerError> {
    to_value(value).map_err(|error| RunnerError::task_invocation(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{
        deploy_history_dir, deploy_latest_path, deploy_report_paths, read_optional_json,
        write_json_report,
    };
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn report_paths_cannot_escape_the_reports_root() {
        let root = Path::new("/repo");
        // `safe_path_component` is `effigy_state::paths`'s, so a `..` segment
        // stays a file name instead of walking out of the report tree.
        assert_eq!(
            deploy_latest_path(root, "../../etc"),
            root.join(".effigy/reports/deploy/..-..-etc/latest.json")
        );
        assert_eq!(
            deploy_history_dir(root, "a/b"),
            root.join(".effigy/reports/deploy/a-b/history")
        );
        let paths = deploy_report_paths(root, "production", "1700000000-x");
        assert_eq!(
            paths.latest_path,
            root.join(".effigy/reports/deploy/production/latest.json")
        );
        assert_eq!(
            paths.history_path,
            root.join(".effigy/reports/deploy/production/history/1700000000-x.json")
        );
    }

    #[test]
    fn a_written_report_is_read_back_as_the_same_document() {
        let root = tempfile::tempdir().expect("temp dir");
        let paths = deploy_report_paths(root.path(), "production", "attempt");
        write_json_report(
            root.path(),
            &[&paths.latest_path, &paths.history_path],
            &json!({ "schema": "effigy.deploy.plan.v1" }),
        )
        .expect("write both report copies");

        assert_eq!(
            read_optional_json(&paths.latest_path).expect("read latest"),
            Some(json!({ "schema": "effigy.deploy.plan.v1" }))
        );
        assert_eq!(
            read_optional_json(&paths.history_path).expect("read history"),
            Some(json!({ "schema": "effigy.deploy.plan.v1" }))
        );
        // An absent report is absent, not an empty document.
        assert_eq!(
            read_optional_json(&root.path().join("absent.json")).expect("absent report"),
            None
        );
    }
}
