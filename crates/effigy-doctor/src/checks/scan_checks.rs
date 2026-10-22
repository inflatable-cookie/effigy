use super::super::{duplicate_blocks, god_files};
use super::definitions::DoctorCheckContext;
use crate::DoctorState;

pub(super) fn run_god_files_check(context: &DoctorCheckContext<'_>, state: &mut DoctorState) {
    god_files::check_god_files(
        context.resolved_root,
        &context.manifest.parsed_catalogs,
        state,
    );
}

pub(super) fn run_duplicate_blocks_check(
    context: &DoctorCheckContext<'_>,
    state: &mut DoctorState,
) {
    duplicate_blocks::check_duplicate_blocks(
        context.resolved_root,
        &context.manifest.parsed_catalogs,
        state,
    );
}
