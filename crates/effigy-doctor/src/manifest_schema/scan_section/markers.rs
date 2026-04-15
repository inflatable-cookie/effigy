use super::*;

pub(super) fn validate_attention_markers_section(
    context: &mut SchemaContext<'_, '_>,
    value: Option<&Value>,
    section_path: &str,
) {
    let Some(table) = section_table(context, value, section_path, ATTENTION_MARKER_KEYS) else {
        return;
    };

    for field in ["warning", "high", "critical"] {
        validate_string_or_array_field(context, table.get(field), &field_path(section_path, field));
    }

    validate_common_scan_fields(context, table, section_path);
}

pub(super) fn validate_stale_suppressions_section(
    context: &mut SchemaContext<'_, '_>,
    value: Option<&Value>,
    section_path: &str,
) {
    let Some(table) = section_table(context, value, section_path, STALE_SUPPRESSION_KEYS) else {
        return;
    };

    for field in ["warning", "high", "critical"] {
        validate_string_or_array_field(context, table.get(field), &field_path(section_path, field));
    }

    validate_common_scan_fields(context, table, section_path);
}
