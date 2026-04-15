use toml::{map::Map, Value};

use super::diagnostics::SchemaContext;
use super::tables::validate_allowed_keys;
use super::values::{
    validate_non_empty_string_or_array_of_non_empty_strings, validate_optional_boolean_field,
    validate_optional_float_field, validate_optional_integer_field,
    validate_optional_non_empty_string_field,
};

mod common;
mod keys;
mod markers;
mod thresholds;

use common::{
    field_path, section_table, validate_common_scan_fields, validate_string_or_array_field,
};
use keys::{
    ATTENTION_MARKER_KEYS, COMMENT_RATIO_KEYS, GENERATED_IN_SRC_KEYS, STALE_SUPPRESSION_KEYS,
    THRESHOLD_SCAN_KEYS,
};
use markers::{validate_attention_markers_section, validate_stale_suppressions_section};
use thresholds::{
    validate_comment_ratio_section, validate_duplicate_blocks_section,
    validate_generated_in_src_section, validate_threshold_scan_section,
};

pub(super) fn validate_scan_section(context: &mut SchemaContext<'_, '_>, scan: &Value) {
    let Some(scan_table) = scan.as_table() else {
        context.unsupported_value("scan", SchemaContext::value_type(scan), "expected table");
        return;
    };
    validate_allowed_keys(
        context,
        "scan",
        scan_table,
        &[
            "god_files",
            "duplicate_blocks",
            "comment_ratio",
            "generated_assets",
            "generated_in_src",
            "attention_markers",
            "stale_suppressions",
        ],
    );
    validate_threshold_scan_section(context, scan_table.get("god_files"), "scan.god_files");
    validate_duplicate_blocks_section(
        context,
        scan_table.get("duplicate_blocks"),
        "scan.duplicate_blocks",
    );
    validate_comment_ratio_section(
        context,
        scan_table.get("comment_ratio"),
        "scan.comment_ratio",
    );
    validate_threshold_scan_section(
        context,
        scan_table.get("generated_assets"),
        "scan.generated_assets",
    );
    validate_generated_in_src_section(
        context,
        scan_table.get("generated_in_src"),
        "scan.generated_in_src",
    );
    validate_attention_markers_section(
        context,
        scan_table.get("attention_markers"),
        "scan.attention_markers",
    );
    validate_stale_suppressions_section(
        context,
        scan_table.get("stale_suppressions"),
        "scan.stale_suppressions",
    );
}
