use super::super::super::DoctorState;
use super::definitions::{DoctorCheckContext, DoctorCheckDefinition};

pub(super) fn run_registered_checks(
    checks: &[DoctorCheckDefinition],
    context: &DoctorCheckContext<'_>,
    state: &mut DoctorState,
) {
    for_each_check(checks, |check| {
        // Keep a stable execution identity available for future tracing hooks.
        let _check_name = check.name;
        (check.run)(context, state);
    });
}

pub(super) fn for_each_check<F>(checks: &[DoctorCheckDefinition], mut visit: F)
where
    F: FnMut(&DoctorCheckDefinition),
{
    for check in checks {
        visit(check);
    }
}
