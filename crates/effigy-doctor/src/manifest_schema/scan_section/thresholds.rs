use super::*;

pub(super) fn validate_threshold_scan_section(
    context: &mut SchemaContext<'_, '_>,
    value: Option<&Value>,
    section_path: &str,
) {
    let Some(table) = section_table(context, value, section_path, THRESHOLD_SCAN_KEYS) else {
        return;
    };

    for field in ["threshold", "warn", "high", "critical"] {
        validate_optional_integer_field(
            context,
            table.get(field),
            &field_path(section_path, field),
        );
    }

    validate_common_scan_fields(context, table, section_path);
}

pub(super) fn validate_duplicate_blocks_section(
    context: &mut SchemaContext<'_, '_>,
    value: Option<&Value>,
    section_path: &str,
) {
    let Some(table) = section_table(context, value, section_path, THRESHOLD_SCAN_KEYS) else {
        return;
    };

    for field in ["threshold", "warn", "high", "critical", "min_occurrences"] {
        validate_optional_integer_field(
            context,
            table.get(field),
            &field_path(section_path, field),
        );
    }

    validate_common_scan_fields(context, table, section_path);
}
