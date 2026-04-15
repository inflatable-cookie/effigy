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

pub(super) fn validate_generated_in_src_section(
    context: &mut SchemaContext<'_, '_>,
    value: Option<&Value>,
    section_path: &str,
) {
    let Some(table) = section_table(context, value, section_path, GENERATED_IN_SRC_KEYS) else {
        return;
    };

    for field in [
        "threshold",
        "warn",
        "warn_bytes",
        "high",
        "high_bytes",
        "critical",
        "critical_bytes",
    ] {
        validate_optional_integer_field(
            context,
            table.get(field),
            &field_path(section_path, field),
        );
    }
    for field in ["source_root", "source_roots"] {
        validate_string_or_array_field(context, table.get(field), &field_path(section_path, field));
    }

    validate_common_scan_fields(context, table, section_path);
}

pub(super) fn validate_comment_ratio_section(
    context: &mut SchemaContext<'_, '_>,
    value: Option<&Value>,
    section_path: &str,
) {
    let Some(table) = section_table(context, value, section_path, COMMENT_RATIO_KEYS) else {
        return;
    };

    for field in ["threshold", "warn", "high", "critical"] {
        validate_optional_float_field(context, table.get(field), &field_path(section_path, field));
    }
    validate_optional_integer_field(
        context,
        table.get("min_code_lines"),
        &field_path(section_path, "min_code_lines"),
    );

    validate_common_scan_fields(context, table, section_path);
}
