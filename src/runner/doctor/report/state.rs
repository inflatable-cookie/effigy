use std::collections::HashMap;

use super::{DoctorFinding, DoctorFixAction, DoctorReport, DoctorSeverity, DoctorSummary};

pub(in crate::runner) struct DoctorState {
    pub(in crate::runner) findings: Vec<DoctorFinding>,
    pub(in crate::runner) statuses: HashMap<String, DoctorSeverity>,
    pub(in crate::runner) fixes: Vec<DoctorFixAction>,
}

impl DoctorState {
    pub(in crate::runner) fn new() -> Self {
        Self {
            findings: Vec::new(),
            statuses: effigy_doctor::DoctorState::new().statuses,
            fixes: Vec::new(),
        }
    }

    pub(in crate::runner) fn add_finding(&mut self, finding: DoctorFinding) {
        add_state_finding(&mut self.findings, &mut self.statuses, finding);
    }

    pub(in crate::runner) fn add_check_finding(
        &mut self,
        check_id: &str,
        severity: DoctorSeverity,
        evidence: impl Into<String>,
        remediation: impl Into<String>,
        fixable: bool,
    ) {
        self.add_finding(DoctorFinding {
            check_id: check_id.to_owned(),
            severity,
            evidence: evidence.into(),
            remediation: remediation.into(),
            fixable,
        });
    }

    pub(in crate::runner) fn add_check_info(
        &mut self,
        check_id: &str,
        evidence: impl Into<String>,
        remediation: impl Into<String>,
    ) {
        self.add_check_finding(check_id, DoctorSeverity::Info, evidence, remediation, false);
    }

    pub(in crate::runner) fn add_check_warning(
        &mut self,
        check_id: &str,
        evidence: impl Into<String>,
        remediation: impl Into<String>,
    ) {
        self.add_check_finding(
            check_id,
            DoctorSeverity::Warning,
            evidence,
            remediation,
            false,
        );
    }

    pub(in crate::runner) fn add_check_fixable_warning(
        &mut self,
        check_id: &str,
        evidence: impl Into<String>,
        remediation: impl Into<String>,
    ) {
        self.add_check_finding(
            check_id,
            DoctorSeverity::Warning,
            evidence,
            remediation,
            true,
        );
    }

    pub(in crate::runner) fn add_check_error(
        &mut self,
        check_id: &str,
        evidence: impl Into<String>,
        remediation: impl Into<String>,
    ) {
        self.add_check_finding(
            check_id,
            DoctorSeverity::Error,
            evidence,
            remediation,
            false,
        );
    }

    pub(in crate::runner) fn summarize(&self) -> DoctorSummary {
        effigy_doctor::DoctorState {
            findings: self.findings.clone(),
            statuses: self.statuses.clone(),
            fixes: self.fixes.clone(),
        }
        .summarize()
    }

    pub(in crate::runner) fn finalize_fix_actions(&mut self, should_fix: bool) {
        effigy_doctor::finalize_fix_actions(&mut self.fixes, should_fix);
    }

    pub(in crate::runner) fn into_report(
        self,
        resolved_root: String,
        summary: DoctorSummary,
        root_evidence: Vec<String>,
        root_warnings: Vec<String>,
    ) -> DoctorReport {
        effigy_doctor::DoctorReport {
            resolved_root,
            summary,
            findings: self.findings,
            fixes: self.fixes,
            root_evidence,
            root_warnings,
        }
    }
}

impl effigy_doctor::FindingSink for DoctorState {
    fn add_check_error(&mut self, check_id: &str, evidence: String, remediation: String) {
        DoctorState::add_check_error(self, check_id, evidence, remediation);
    }
}

fn add_state_finding(
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
    finding: DoctorFinding,
) {
    let status = statuses
        .entry(finding.check_id.clone())
        .or_insert(DoctorSeverity::Info);
    if finding.severity > *status {
        *status = finding.severity;
    }
    findings.push(finding);
}
