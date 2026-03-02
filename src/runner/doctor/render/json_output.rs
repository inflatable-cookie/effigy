use serde_json::json;

use super::super::{DoctorReport, RunnerError};

pub(super) fn render_json(report: &DoctorReport) -> Result<String, RunnerError> {
    let findings = report
        .findings
        .iter()
        .map(|finding| {
            json!({
                "check_id": finding.check_id,
                "severity": finding.severity.as_str(),
                "evidence": finding.evidence,
                "remediation": finding.remediation,
                "fixable": finding.fixable,
            })
        })
        .collect::<Vec<serde_json::Value>>();
    let payload = json!({
        "schema": "effigy.doctor.v1",
        "schema_version": 1,
        "ok": report.summary.error == 0,
        "summary": {
            "checks": report.summary.checks,
            "pass": report.summary.pass,
            "warning": report.summary.warning,
            "error": report.summary.error,
        },
        "findings": findings,
        "fixes": report.fixes.iter().map(|fix| {
            json!({
                "fix_id": fix.fix_id,
                "status": fix.status.as_str(),
                "detail": fix.detail,
            })
        }).collect::<Vec<serde_json::Value>>(),
        "root_resolution": {
            "evidence": report.root_evidence,
            "warnings": report.root_warnings,
        }
    });
    serde_json::to_string_pretty(&payload)
        .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")))
}
