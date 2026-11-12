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

#[cfg(test)]
mod tests {
    use super::{
        validate_provider_phase_report, DeployProviderCheckStatus, DeployProviderPhaseReport,
        DeployProviderReportStatus,
    };

    fn report(phase: &str, status: DeployProviderReportStatus) -> DeployProviderPhaseReport {
        DeployProviderPhaseReport {
            schema: "effigy.deploy-provider.report.v1".to_owned(),
            phase: phase.to_owned(),
            provider: "acme".to_owned(),
            status,
            checks: vec![super::DeployProviderPhaseCheck {
                name: "plan".to_owned(),
                status: DeployProviderCheckStatus::Planned,
                target: None,
                message: None,
            }],
            warnings: Vec::new(),
            blockers: Vec::new(),
            files: Vec::new(),
        }
    }

    #[test]
    fn a_report_must_name_its_schema_provider_and_phase() {
        let mut wrong_schema = report("export", DeployProviderReportStatus::Succeeded);
        wrong_schema.schema = "effigy.deploy-provider.report.v2".to_owned();
        assert!(
            validate_provider_phase_report("acme", "export", &wrong_schema)
                .expect_err("unsupported schema")
                .to_string()
                .contains("has unsupported schema")
        );

        let mut wrong_provider = report("export", DeployProviderReportStatus::Succeeded);
        wrong_provider.provider = "other".to_owned();
        assert!(
            validate_provider_phase_report("acme", "export", &wrong_provider)
                .expect_err("another provider")
                .to_string()
                .contains("report declares provider `other`")
        );

        let phase = report("status", DeployProviderReportStatus::Ok);
        assert!(validate_provider_phase_report("acme", "export", &phase)
            .expect_err("another phase")
            .to_string()
            .contains("report declares phase `status`"));
    }

    #[test]
    fn a_status_a_phase_cannot_return_is_refused() {
        // An export ends succeeded, warned, failed or skipped, so a plan-only
        // `ok` is not an export outcome; an apply ends succeeded, failed or
        // skipped, so a preview `planned` is not an apply outcome either.
        assert!(validate_provider_phase_report(
            "acme",
            "export",
            &report("export", DeployProviderReportStatus::Succeeded)
        )
        .is_ok());
        let error = validate_provider_phase_report(
            "acme",
            "export",
            &report("export", DeployProviderReportStatus::Ok),
        )
        .expect_err("ok is not an export outcome");
        assert!(
            error
                .to_string()
                .contains("export report returned unsupported status `ok`"),
            "got: {error}"
        );
        let error = validate_provider_phase_report(
            "acme",
            "apply",
            &report("apply", DeployProviderReportStatus::Planned),
        )
        .expect_err("planned is not an apply outcome");
        assert!(
            error
                .to_string()
                .contains("apply report returned unsupported status `planned`"),
            "got: {error}"
        );
        // A preflight is a preview: `planned` is exactly its outcome.
        assert!(validate_provider_phase_report(
            "acme",
            "preflight",
            &report("preflight", DeployProviderReportStatus::Planned)
        )
        .is_ok());
    }
}
