use super::super::progress::{DoctorProgressReporter, ScanOutcome};
use super::definitions::{DoctorCheckContext, DoctorCheckDefinition};
use crate::DoctorState;
use std::time::Instant;

pub(super) fn run_registered_checks(
    checks: &[DoctorCheckDefinition],
    context: &DoctorCheckContext<'_>,
    state: &mut DoctorState,
    mut progress: Option<&mut DoctorProgressReporter>,
) -> bool {
    for (index, check) in checks.iter().enumerate() {
        if context
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            state.record_budget_exhausted(check.name, std::time::Duration::ZERO);
            for skipped in &checks[index + 1..] {
                state.record_skipped_check(skipped.name);
            }
            return false;
        }
        let _check_name = check.name;
        if let (Some(progress), Some(label)) = (progress.as_deref_mut(), check.progress_label) {
            progress.start_scan(label);
        }
        let findings_before = state.findings.len();
        let started = Instant::now();
        (check.run)(context, state);
        let elapsed = started.elapsed();
        let stopped = context
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline);
        if let (Some(progress), Some(label)) = (progress.as_deref_mut(), check.progress_label) {
            let outcome = if stopped {
                ScanOutcome::Stopped("stopped at the time budget")
            } else {
                ScanOutcome::from_findings(&state.findings[findings_before..])
            };
            progress.finish_scan(label, &outcome);
        }
        if stopped {
            state.record_budget_exhausted(check.name, elapsed);
            for skipped in &checks[index + 1..] {
                state.record_skipped_check(skipped.name);
            }
            return false;
        }
        state.record_completed_check(check.name, elapsed);
    }
    true
}
