#[path = "options/defaults.rs"]
mod defaults;
#[path = "options/loading/mod.rs"]
mod loading;
#[path = "options/validation.rs"]
mod validation;

pub use loading::{
    catalog_scan_roots, doctor_attention_marker_options, doctor_comment_ratio_options,
    doctor_dead_code_options, doctor_duplicate_block_options, doctor_generated_asset_options,
    doctor_generated_in_src_options, doctor_god_file_options, doctor_stale_suppression_options,
    doctor_validation_gap_options, load_root_attention_marker_options,
    load_root_boundary_violation_options, load_root_comment_ratio_options,
    load_root_dead_code_options, load_root_duplicate_block_options,
    load_root_generated_asset_options, load_root_generated_in_src_options,
    load_root_god_file_options, load_root_stale_suppression_options,
    load_root_validation_gap_options,
};
