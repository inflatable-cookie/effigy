use super::super::{conflicts, environment, health, references};
use super::definitions::DoctorCheckContext;
use crate::DoctorState;

pub(super) fn run_manifest_conflicts_check(
    context: &DoctorCheckContext<'_>,
    state: &mut DoctorState,
) {
    conflicts::check_manifest_alias_conflicts(&context.manifest.parsed_catalogs, state);
}

pub(super) fn run_environment_tools_check(
    context: &DoctorCheckContext<'_>,
    state: &mut DoctorState,
) {
    environment::check_environment_tools(
        context.resolved_root,
        &context.manifest.parsed_catalogs,
        context.manifest.preferred_js_pm,
        state,
    );
}

pub(super) fn run_task_references_check(context: &DoctorCheckContext<'_>, state: &mut DoctorState) {
    references::check_task_references(&context.manifest.parsed_catalogs, state);
}

pub(super) fn run_draft_lifecycle_check(context: &DoctorCheckContext<'_>, state: &mut DoctorState) {
    crate::draft_lifecycle::check_draft_lifecycle(
        &context.manifest.parsed_catalogs,
        effigy_manifest::ManifestDraftDate::today_local(),
        state,
    );
}

pub(super) fn run_health_task_check(context: &DoctorCheckContext<'_>, state: &mut DoctorState) {
    health::check_health_task(
        context.resolved_root,
        &context.manifest.parsed_catalogs,
        state,
        context.ports,
    );
}
