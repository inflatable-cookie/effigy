use super::super::progress::DoctorProgressReporter;
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
        if let Some(progress) = progress.as_deref_mut() {
            if let Some(label) = check.progress_label {
                progress.start_scan(label);
            }
        }
        let started = Instant::now();
        (check.run)(context, state);
        let elapsed = started.elapsed();
        if context
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
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

#[cfg(test)]
pub(super) fn for_each_check<F>(checks: &[DoctorCheckDefinition], mut visit: F)
where
    F: FnMut(&DoctorCheckDefinition),
{
    for check in checks {
        visit(check);
    }
}
