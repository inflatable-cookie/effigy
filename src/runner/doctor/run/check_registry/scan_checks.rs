use super::super::super::report::DoctorState;
use super::super::super::{
    attention_markers, comment_ratio, duplicate_blocks, generated_assets, generated_in_src,
    god_files, stale_suppressions,
};
use super::definitions::DoctorCheckContext;

pub(super) fn run_god_files_check(context: &DoctorCheckContext<'_>, state: &mut DoctorState) {
    god_files::check_god_files(
        context.resolved_root,
        &context.manifest.parsed_catalogs,
        state,
    );
}

pub(super) fn run_generated_assets_check(
    context: &DoctorCheckContext<'_>,
    state: &mut DoctorState,
) {
    generated_assets::check_generated_assets(
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

pub(super) fn run_comment_ratio_check(context: &DoctorCheckContext<'_>, state: &mut DoctorState) {
    comment_ratio::check_comment_ratio(
        context.resolved_root,
        &context.manifest.parsed_catalogs,
        state,
    );
}

pub(super) fn run_generated_in_src_check(
    context: &DoctorCheckContext<'_>,
    state: &mut DoctorState,
) {
    generated_in_src::check_generated_in_src(
        context.resolved_root,
        &context.manifest.parsed_catalogs,
        state,
    );
}

pub(super) fn run_attention_markers_check(
    context: &DoctorCheckContext<'_>,
    state: &mut DoctorState,
) {
    attention_markers::check_attention_markers(
        context.resolved_root,
        &context.manifest.parsed_catalogs,
        state,
    );
}

pub(super) fn run_stale_suppressions_check(
    context: &DoctorCheckContext<'_>,
    state: &mut DoctorState,
) {
    stale_suppressions::check_stale_suppressions(
        context.resolved_root,
        &context.manifest.parsed_catalogs,
        state,
    );
}
