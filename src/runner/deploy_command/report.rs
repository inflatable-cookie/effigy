use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{to_value, Value};

use crate::runner::error::RunnerError;

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
pub(super) struct DeployApplyReport {
    pub(super) schema: String,
    pub(super) schema_version: u8,
    pub(super) deployment_id: String,
    pub(super) env: String,
    pub(super) provider: String,
    pub(super) status: String,
    pub(super) started_at: String,
    pub(super) finished_at: String,
    pub(super) code: DeployCodeRef,
    pub(super) release_policy: DeployReleasePolicyReport,
    pub(super) state: DeployApplyStateReport,
    pub(super) provider_operation: DeployProviderOperationReport,
    pub(super) hooks: Vec<DeployHookResult>,
    pub(super) health_checks: Vec<DeployHealthResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) written_report_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) written_history_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct DeployApplyStateReport {
    pub(super) status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) lineage_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) apply_report_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct DeployProviderOperationReport {
    pub(super) status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) provider_deployment_id: Option<String>,
    pub(super) services: Vec<String>,
    pub(super) warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct DeployHookResult {
    pub(super) stage: String,
    pub(super) task: String,
    pub(super) status: String,
}

#[derive(Debug, Serialize)]
pub(super) struct DeployHealthResult {
    pub(super) service: String,
    pub(super) status: String,
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

#[derive(Debug, Serialize)]
pub(super) struct DeployRedeployReport {
    pub(super) schema: String,
    pub(super) schema_version: u8,
    pub(super) deployment_id: String,
    pub(super) env: String,
    pub(super) provider: String,
    pub(super) status: String,
    pub(super) source_deployment: String,
    pub(super) started_at: String,
    pub(super) finished_at: String,
    pub(super) source_report_path: String,
    pub(super) warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) written_report_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) written_history_path: Option<String>,
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

pub(super) fn path_display(path: &Path, repo_root: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .display()
        .to_string()
}

pub(super) fn safe_path_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}
