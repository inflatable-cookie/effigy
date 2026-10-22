use toml::{map::Map, Value};

use super::diagnostics::SchemaContext;
use super::tables::validate_allowed_keys;
use super::values::{
    validate_non_empty_string_or_array_of_non_empty_strings, validate_optional_boolean_field,
    validate_optional_integer_field, validate_optional_non_empty_string_field,
};

mod common;
mod keys;
mod thresholds;

use common::{field_path, section_table, validate_common_scan_fields};
use keys::THRESHOLD_SCAN_KEYS;
use thresholds::{validate_duplicate_blocks_section, validate_threshold_scan_section};

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
            // Retired scanners keep loading for one release (effigy#274); the
            // manifest names them in its one deprecation notice.
            "attention_markers",
            "comment_ratio",
            "generated_assets",
            "generated_in_src",
            "stale_suppressions",
            "boundary_violations",
            "dead_code",
            "validation_gaps",
        ],
    );
    validate_threshold_scan_section(context, scan_table.get("god_files"), "scan.god_files");
    validate_duplicate_blocks_section(
        context,
        scan_table.get("duplicate_blocks"),
        "scan.duplicate_blocks",
    );
}
