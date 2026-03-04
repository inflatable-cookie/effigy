use std::collections::HashMap;

use super::super::contracts::ALL_CHECK_IDS;
use super::types::{
    DoctorFinding, DoctorFixAction, DoctorFixStatus, DoctorReport, DoctorSeverity, DoctorSummary,
};

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
        if should_fix && self.fixes.is_empty() {
            self.fixes.push(DoctorFixAction {
                fix_id: "manifest.health_task_scaffold".to_owned(),
                status: DoctorFixStatus::Skipped,
                detail: "No safe automatic fixes were applicable.".to_owned(),
            });
        }
    }

    pub(in crate::runner) fn into_report(
        self,
        summary: DoctorSummary,
        root_evidence: Vec<String>,
        root_warnings: Vec<String>,
    ) -> DoctorReport {
        DoctorReport {
            summary,
            findings: self.findings,
            fixes: self.fixes,
            root_evidence,
            root_warnings,
        }
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

fn initialize_statuses() -> HashMap<String, DoctorSeverity> {
    ALL_CHECK_IDS
        .into_iter()
        .map(|id| (id.to_owned(), DoctorSeverity::Info))
        .collect::<HashMap<String, DoctorSeverity>>()
}

fn summarize_statuses(statuses: &HashMap<String, DoctorSeverity>) -> DoctorSummary {
    let mut pass = 0usize;
    let mut warning = 0usize;
    let mut error = 0usize;
    for check in ALL_CHECK_IDS {
        match statuses.get(check).copied().unwrap_or(DoctorSeverity::Info) {
            DoctorSeverity::Info => pass += 1,
            DoctorSeverity::Warning => warning += 1,
            DoctorSeverity::Error => error += 1,
        }
    }
    DoctorSummary {
        checks: ALL_CHECK_IDS.len(),
        pass,
        warning,
        error,
    }
}
