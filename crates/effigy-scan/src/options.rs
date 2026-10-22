#[path = "options/defaults.rs"]
mod defaults;
#[path = "options/loading/mod.rs"]
mod loading;
#[path = "options/validation.rs"]
mod validation;

pub use loading::{
    catalog_scan_roots, doctor_duplicate_block_options, doctor_god_file_options,
    load_root_duplicate_block_options, load_root_god_file_options,
};
