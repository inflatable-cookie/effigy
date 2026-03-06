#[path = "options/defaults.rs"]
mod defaults;
#[path = "options/loading.rs"]
mod loading;
#[path = "options/validation.rs"]
mod validation;

pub(in crate::runner) use loading::{
    catalog_scan_roots, doctor_attention_marker_options, doctor_comment_ratio_options,
    doctor_duplicate_block_options, doctor_generated_asset_options,
    doctor_generated_in_src_options, doctor_god_file_options,
    doctor_stale_suppression_options,
    load_root_attention_marker_options, load_root_comment_ratio_options,
    load_root_duplicate_block_options, load_root_generated_asset_options,
    load_root_generated_in_src_options, load_root_stale_suppression_options,
    load_root_god_file_options,
};
