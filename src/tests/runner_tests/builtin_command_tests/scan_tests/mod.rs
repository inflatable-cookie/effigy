use super::prelude::{
    assert_file_text_contains_all, assert_json_array_field_non_empty, assert_json_string_field_eq,
    assert_output_contains_all, assert_output_excludes_all, fs, parse_json_output_with_schema,
    run_builtin_err, run_builtin_ok, temp_workspace, write_manifest, write_root_manifest,
    RunnerError,
};

mod support;

use support::*;

mod attention_markers;
mod comment_ratio;
mod duplicate_blocks;
mod generated_assets;
mod generated_in_src;
mod god_files;
mod stale_suppressions;
