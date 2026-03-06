use std::collections::HashMap;

use super::super::contracts::ALL_CHECK_IDS;
use super::types::{DoctorFinding, DoctorSeverity, DoctorSummary};

pub(super) fn initialize_statuses() -> HashMap<String, DoctorSeverity> {
    ALL_CHECK_IDS
        .into_iter()
        .map(|id| (id.to_owned(), DoctorSeverity::Info))
        .collect::<HashMap<String, DoctorSeverity>>()
}

pub(super) fn record_finding_status(
    statuses: &mut HashMap<String, DoctorSeverity>,
    finding: &DoctorFinding,
) {
    let status = statuses
        .entry(finding.check_id.clone())
        .or_insert(DoctorSeverity::Info);
    if finding.severity > *status {
        *status = finding.severity;
    }
}

pub(super) fn summarize_statuses(statuses: &HashMap<String, DoctorSeverity>) -> DoctorSummary {
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
