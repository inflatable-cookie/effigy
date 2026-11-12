use serde::{Deserialize, Serialize};

use crate::runner::error::RunnerError;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(in crate::runner::deploy_command) struct DeployProviderPhaseReport {
    pub schema: String,
    pub phase: String,
    pub provider: String,
    pub status: DeployProviderReportStatus,
    #[serde(default)]
    pub checks: Vec<DeployProviderPhaseCheck>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub blockers: Vec<String>,
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(in crate::runner::deploy_command) struct DeployProviderPhaseCheck {
    pub name: String,
    pub status: DeployProviderCheckStatus,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(in crate::runner::deploy_command) enum DeployProviderReportStatus {
    Planned,
    Ok,
    Warning,
    Blocked,
    Succeeded,
    Failed,
    Skipped,
}

impl DeployProviderReportStatus {
    pub(in crate::runner::deploy_command) fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Ok => "ok",
            Self::Warning => "warning",
            Self::Blocked => "blocked",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }

    pub(in crate::runner::deploy_command) fn allows_preflight_progress(self) -> bool {
        matches!(self, Self::Planned | Self::Ok)
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(in crate::runner::deploy_command) enum DeployProviderCheckStatus {
    Planned,
    Ok,
    Warning,
    Blocked,
    Succeeded,
    Failed,
    Skipped,
}

impl DeployProviderCheckStatus {
    pub(in crate::runner::deploy_command) fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Ok => "ok",
            Self::Warning => "warning",
            Self::Blocked => "blocked",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}

/// Resolve a provider package. A git source can fetch for seconds on a cold
/// cache, so it draws a live step: `resolving deploy provider acme (offline)`,
/// waiting on the exact source and pin. A path source resolves at once and

pub(super) fn validate_provider_phase_report(
    provider_name: &str,
    phase: &str,
    report: &DeployProviderPhaseReport,
) -> Result<(), RunnerError> {
    if report.schema != "effigy.deploy-provider.report.v1" {
        return Err(RunnerError::task_invocation(format!(
            "deploy provider `{provider_name}` {phase} report has unsupported schema `{}`",
            report.schema
        )));
    }
    if report.provider != provider_name {
        return Err(RunnerError::task_invocation(format!(
            "deploy provider `{provider_name}` {phase} report declares provider `{}`",
            report.provider
        )));
    }
    if report.phase != phase {
        return Err(RunnerError::task_invocation(format!(
            "deploy provider `{provider_name}` {phase} report declares phase `{}`",
            report.phase
        )));
    }
    validate_provider_report_status(provider_name, phase, report)?;
    Ok(())
}

pub(super) fn validate_provider_report_status(
    provider_name: &str,
    phase: &str,
    report: &DeployProviderPhaseReport,
) -> Result<(), RunnerError> {
    let valid = match phase {
        "preflight" => matches!(
            report.status,
            DeployProviderReportStatus::Planned
                | DeployProviderReportStatus::Ok
                | DeployProviderReportStatus::Warning
                | DeployProviderReportStatus::Blocked
                | DeployProviderReportStatus::Failed
                | DeployProviderReportStatus::Skipped
        ),
        "apply" => matches!(
            report.status,
            DeployProviderReportStatus::Succeeded
                | DeployProviderReportStatus::Failed
                | DeployProviderReportStatus::Skipped
        ),
        "status" => matches!(
            report.status,
            DeployProviderReportStatus::Planned
                | DeployProviderReportStatus::Ok
                | DeployProviderReportStatus::Warning
                | DeployProviderReportStatus::Blocked
                | DeployProviderReportStatus::Failed
                | DeployProviderReportStatus::Skipped
        ),
        "export" => matches!(
            report.status,
            DeployProviderReportStatus::Planned
                | DeployProviderReportStatus::Succeeded
                | DeployProviderReportStatus::Warning
                | DeployProviderReportStatus::Failed
                | DeployProviderReportStatus::Skipped
        ),
        _ => true,
    };
    if valid {
        return Ok(());
    }
    Err(RunnerError::task_invocation(format!(
        "deploy provider `{provider_name}` {phase} report returned unsupported status `{}`",
        report.status.as_str()
    )))
}

