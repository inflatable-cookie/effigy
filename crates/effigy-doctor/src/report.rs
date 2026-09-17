use std::collections::HashMap;
use std::time::Duration;

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
    pub mode: DoctorMode,
    pub scopes: Vec<DoctorScope>,
    pub budget_ms: Option<u64>,
    pub elapsed_ms: u64,
    pub complete: bool,
    pub timeout_phase: Option<String>,
    pub check_runs: Vec<DoctorCheckRun>,
    pub cache: DoctorCacheSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorMode {
    Fast,
    Deep,
}

impl DoctorMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Deep => "deep",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorScope {
    pub alias: String,
    pub root: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorCheckRunState {
    Complete,
    Cached,
    Skipped,
    BudgetExhausted,
}

impl DoctorCheckRunState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Cached => "cached",
            Self::Skipped => "skipped",
            Self::BudgetExhausted => "budget-exhausted",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorCheckRun {
    pub name: String,
    pub state: DoctorCheckRunState,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DoctorCacheSummary {
    pub hits: usize,
    pub misses: usize,
    pub invalid_entries: usize,
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
    pub check_runs: Vec<DoctorCheckRun>,
    complete: bool,
    timeout_phase: Option<String>,
    cache: DoctorCacheSummary,
}

impl Default for DoctorState {
    fn default() -> Self {
        Self::new()
    }
}

impl DoctorState {
    pub fn new() -> Self {
        Self {
            findings: Vec::new(),
            statuses: initialize_statuses(),
            fixes: Vec::new(),
            check_runs: Vec::new(),
            complete: true,
            timeout_phase: None,
            cache: DoctorCacheSummary::default(),
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

    pub fn record_completed_check(&mut self, name: &str, duration: Duration) {
        self.check_runs.push(DoctorCheckRun {
            name: name.to_owned(),
            state: DoctorCheckRunState::Complete,
            duration_ms: duration.as_millis().min(u128::from(u64::MAX)) as u64,
        });
    }

    pub fn record_cached_check(&mut self, name: &str, duration: Duration) {
        self.check_runs.push(DoctorCheckRun {
            name: name.to_owned(),
            state: DoctorCheckRunState::Cached,
            duration_ms: duration.as_millis().min(u128::from(u64::MAX)) as u64,
        });
    }

    pub fn record_skipped_check(&mut self, name: &str) {
        self.check_runs.push(DoctorCheckRun {
            name: name.to_owned(),
            state: DoctorCheckRunState::Skipped,
            duration_ms: 0,
        });
    }

    pub fn record_budget_exhausted(&mut self, name: &str, duration: Duration) {
        self.complete = false;
        self.timeout_phase = Some(name.to_owned());
        self.check_runs.push(DoctorCheckRun {
            name: name.to_owned(),
            state: DoctorCheckRunState::BudgetExhausted,
            duration_ms: duration.as_millis().min(u128::from(u64::MAX)) as u64,
        });
    }

    pub fn is_complete(&self) -> bool {
        self.complete
    }

    pub fn record_cache_summary(&mut self, hits: usize, misses: usize, invalid_entries: usize) {
        self.cache.hits += hits;
        self.cache.misses += misses;
        self.cache.invalid_entries += invalid_entries;
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
            mode: DoctorMode::Fast,
            scopes: Vec::new(),
            budget_ms: None,
            elapsed_ms: 0,
            complete: self.complete,
            timeout_phase: self.timeout_phase,
            check_runs: self.check_runs,
            cache: self.cache,
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
