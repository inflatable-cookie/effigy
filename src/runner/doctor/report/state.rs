use std::collections::HashMap;

use super::finalize::{build_report, finalize_fix_actions};
use super::summary::{initialize_statuses, record_finding_status, summarize_statuses};
use super::types::{DoctorFinding, DoctorFixAction, DoctorReport, DoctorSeverity, DoctorSummary};

pub(in crate::runner) struct DoctorState {
    pub(in crate::runner) findings: Vec<DoctorFinding>,
    pub(in crate::runner) statuses: HashMap<String, DoctorSeverity>,
    pub(in crate::runner) fixes: Vec<DoctorFixAction>,
}

impl DoctorState {
    pub(in crate::runner) fn new() -> Self {
        Self {
            findings: Vec::new(),
            statuses: initialize_statuses(),
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
        summarize_statuses(&self.statuses)
    }

    pub(in crate::runner) fn finalize_fix_actions(&mut self, should_fix: bool) {
        finalize_fix_actions(&mut self.fixes, should_fix);
    }

    pub(in crate::runner) fn into_report(
        self,
        resolved_root: String,
        summary: DoctorSummary,
        root_evidence: Vec<String>,
        root_warnings: Vec<String>,
    ) -> DoctorReport {
        build_report(
            resolved_root,
            summary,
            self.findings,
            self.fixes,
            root_evidence,
            root_warnings,
        )
    }
}

fn add_state_finding(
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
    finding: DoctorFinding,
) {
    record_finding_status(statuses, &finding);
    findings.push(finding);
}
