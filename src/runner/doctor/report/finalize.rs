use super::types::{DoctorFixAction, DoctorFixStatus, DoctorReport, DoctorSummary};

pub(super) fn finalize_fix_actions(fixes: &mut Vec<DoctorFixAction>, should_fix: bool) {
    if should_fix && fixes.is_empty() {
        fixes.push(DoctorFixAction {
            fix_id: "manifest.health_task_scaffold".to_owned(),
            status: DoctorFixStatus::Skipped,
            detail: "No safe automatic fixes were applicable.".to_owned(),
        });
    }
}

pub(super) fn build_report(
    resolved_root: String,
    summary: DoctorSummary,
    findings: Vec<super::types::DoctorFinding>,
    fixes: Vec<DoctorFixAction>,
    root_evidence: Vec<String>,
    root_warnings: Vec<String>,
) -> DoctorReport {
    DoctorReport {
        resolved_root,
        summary,
        findings,
        fixes,
        root_evidence,
        root_warnings,
    }
}
