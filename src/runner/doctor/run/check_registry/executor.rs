use super::super::super::progress::DoctorProgressReporter;
use super::super::super::report::DoctorState;
use super::definitions::{DoctorCheckContext, DoctorCheckDefinition};

pub(super) fn run_registered_checks(
    checks: &[DoctorCheckDefinition],
    context: &DoctorCheckContext<'_>,
    state: &mut DoctorState,
    mut progress: Option<&mut DoctorProgressReporter>,
) {
    for_each_check(checks, |check| {
        let _check_name = check.name;
        if let Some(progress) = progress.as_deref_mut() {
            if let Some(label) = check.progress_label {
                progress.start_scan(label);
            }
        }
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
