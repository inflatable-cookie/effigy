use std::collections::HashMap;

use crate::{DoctorFinding, DoctorSeverity, FindingSink, ALL_CHECK_IDS};

#[derive(Debug, Clone)]
pub struct DoctorSummary {
    pub checks: usize,
    pub pass: usize,
    pub warning: usize,
    pub error: usize,
}

#[derive(Debug, Clone)]
pub struct DoctorReport {
    pub resolved_root: String,
    pub summary: DoctorSummary,
    pub findings: Vec<DoctorFinding>,
    pub fixes: Vec<DoctorFixAction>,
    pub root_evidence: Vec<String>,
    pub root_warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorFixStatus {
    Applied,
    Skipped,
}

impl DoctorFixStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DoctorFixAction {
    pub fix_id: String,
    pub status: DoctorFixStatus,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct DoctorState {
    pub findings: Vec<DoctorFinding>,
    pub statuses: HashMap<String, DoctorSeverity>,
    pub fixes: Vec<DoctorFixAction>,
}

impl DoctorState {
    pub fn new() -> Self {
        Self {
            findings: Vec::new(),
            statuses: initialize_statuses(),
            fixes: Vec::new(),
        }
    }

    pub fn add_finding(&mut self, finding: DoctorFinding) {
        add_state_finding(&mut self.findings, &mut self.statuses, finding);
    }

    pub fn add_check_finding(
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

    pub fn add_check_info(
        &mut self,
        check_id: &str,
        evidence: impl Into<String>,
        remediation: impl Into<String>,
    ) {
        self.add_check_finding(check_id, DoctorSeverity::Info, evidence, remediation, false);
    }

    pub fn add_check_warning(
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

    pub fn add_check_fixable_warning(
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

    pub fn add_check_error(
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

    pub fn summarize(&self) -> DoctorSummary {
        summarize_statuses(&self.statuses)
    }

    pub fn finalize_fix_actions(&mut self, should_fix: bool) {
        finalize_fix_actions(&mut self.fixes, should_fix);
    }

    pub fn into_report(
        self,
        resolved_root: String,
        summary: DoctorSummary,
        root_evidence: Vec<String>,
        root_warnings: Vec<String>,
    ) -> DoctorReport {
        DoctorReport {
            resolved_root,
            summary,
            findings: self.findings,
            fixes: self.fixes,
            root_evidence,
            root_warnings,
        }
    }
}

impl FindingSink for DoctorState {
    fn add_check_error(&mut self, check_id: &str, evidence: String, remediation: String) {
        DoctorState::add_check_error(self, check_id, evidence, remediation);
    }
}

pub fn finalize_fix_actions(fixes: &mut Vec<DoctorFixAction>, should_fix: bool) {
    if should_fix && fixes.is_empty() {
        fixes.push(DoctorFixAction {
            fix_id: "manifest.health_task_scaffold".to_owned(),
            status: DoctorFixStatus::Skipped,
            detail: "No safe automatic fixes were applicable.".to_owned(),
        });
    }
}

fn initialize_statuses() -> HashMap<String, DoctorSeverity> {
    ALL_CHECK_IDS
        .into_iter()
        .map(|id| (id.to_owned(), DoctorSeverity::Info))
        .collect::<HashMap<String, DoctorSeverity>>()
}

fn record_finding_status(statuses: &mut HashMap<String, DoctorSeverity>, finding: &DoctorFinding) {
    let status = statuses
        .entry(finding.check_id.clone())
        .or_insert(DoctorSeverity::Info);
    if finding.severity > *status {
        *status = finding.severity;
    }
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

fn add_state_finding(
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
    finding: DoctorFinding,
) {
    record_finding_status(statuses, &finding);
    findings.push(finding);
}
