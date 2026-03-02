use std::collections::HashMap;

use super::{LoadedCatalog, ManifestJsPackageManager};

const CHECK_IDS: [&str; 9] = [
    "workspace.root-resolution",
    "environment.tools.required",
    "manifest.parse",
    "manifest.schema.unsupported_key",
    "manifest.schema.unsupported_value",
    "manifest.conflicts",
    "tasks.references.resolve",
    "health.task.discovery",
    "health.task.execute",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::runner) enum DoctorSeverity {
    Info,
    Warning,
    Error,
}

impl DoctorSeverity {
    pub(in crate::runner) fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    pub(in crate::runner) fn to_notice_level(self) -> crate::ui::NoticeLevel {
        match self {
            Self::Info => crate::ui::NoticeLevel::Info,
            Self::Warning => crate::ui::NoticeLevel::Warning,
            Self::Error => crate::ui::NoticeLevel::Error,
        }
    }

    pub(in crate::runner) fn rank(self) -> u8 {
        match self {
            Self::Error => 3,
            Self::Warning => 2,
            Self::Info => 1,
        }
    }
}

#[derive(Debug, Clone)]
pub(in crate::runner) struct DoctorFinding {
    pub(in crate::runner) check_id: String,
    pub(in crate::runner) severity: DoctorSeverity,
    pub(in crate::runner) evidence: String,
    pub(in crate::runner) remediation: String,
    pub(in crate::runner) fixable: bool,
}

#[derive(Debug, Clone)]
pub(in crate::runner) struct DoctorSummary {
    pub(in crate::runner) checks: usize,
    pub(in crate::runner) pass: usize,
    pub(in crate::runner) warning: usize,
    pub(in crate::runner) error: usize,
}

#[derive(Debug, Clone)]
pub(in crate::runner) struct DoctorReport {
    pub(in crate::runner) summary: DoctorSummary,
    pub(in crate::runner) findings: Vec<DoctorFinding>,
    pub(in crate::runner) fixes: Vec<DoctorFixAction>,
    pub(in crate::runner) root_evidence: Vec<String>,
    pub(in crate::runner) root_warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) enum DoctorFixStatus {
    Applied,
    Skipped,
}

impl DoctorFixStatus {
    pub(in crate::runner) fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Clone)]
pub(in crate::runner) struct DoctorFixAction {
    pub(in crate::runner) fix_id: String,
    pub(in crate::runner) status: DoctorFixStatus,
    pub(in crate::runner) detail: String,
}

pub(in crate::runner) struct ManifestSnapshot {
    pub(in crate::runner) manifest_paths: Vec<std::path::PathBuf>,
    pub(in crate::runner) parsed_catalogs: Vec<LoadedCatalog>,
    pub(in crate::runner) preferred_js_pm: Option<ManifestJsPackageManager>,
    pub(in crate::runner) parse_ok_any: bool,
}

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
        add_finding(&mut self.findings, &mut self.statuses, finding);
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

pub(in crate::runner) fn add_finding(
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
    CHECK_IDS
        .into_iter()
        .map(|id| (id.to_owned(), DoctorSeverity::Info))
        .collect::<HashMap<String, DoctorSeverity>>()
}

fn summarize_statuses(statuses: &HashMap<String, DoctorSeverity>) -> DoctorSummary {
    let mut pass = 0usize;
    let mut warning = 0usize;
    let mut error = 0usize;
    for check in CHECK_IDS {
        match statuses.get(check).copied().unwrap_or(DoctorSeverity::Info) {
            DoctorSeverity::Info => pass += 1,
            DoctorSeverity::Warning => warning += 1,
            DoctorSeverity::Error => error += 1,
        }
    }
    DoctorSummary {
        checks: CHECK_IDS.len(),
        pass,
        warning,
        error,
    }
}
