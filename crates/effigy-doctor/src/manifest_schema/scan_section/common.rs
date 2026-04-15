use super::*;

pub(super) fn section_table<'a>(
    context: &mut SchemaContext<'_, '_>,
    value: Option<&'a Value>,
    section_path: &str,
    allowed_keys: &[&str],
) -> Option<&'a Map<String, Value>> {
    let value = value?;
    let Some(table) = value.as_table() else {
        context.unsupported_value(
            section_path,
            SchemaContext::value_type(value),
            "expected table",
        );
        return None;
    };
    validate_allowed_keys(context, section_path, table, allowed_keys);
    Some(table)
}

pub(super) fn validate_common_scan_fields(
    context: &mut SchemaContext<'_, '_>,
    table: &Map<String, Value>,
    section_path: &str,
) {
    for field in ["fail_on_findings", "respect_gitignore", "doctor"] {
        validate_optional_boolean_field(
            context,
            table.get(field),
            &field_path(section_path, field),
        );
    }

    for field in ["include", "exclude"] {
        validate_string_or_array_field(context, table.get(field), &field_path(section_path, field));
    }

    validate_format_field(
        context,
        table.get("format"),
        &field_path(section_path, "format"),
    );
    validate_out_field(context, table.get("out"), &field_path(section_path, "out"));
}

pub(super) fn validate_string_or_array_field(
    context: &mut SchemaContext<'_, '_>,
    value: Option<&Value>,
    key_path: &str,
) {
    if let Some(value) = value {
        validate_non_empty_string_or_array_of_non_empty_strings(
            context,
            key_path,
            value,
            "expected string or array of strings",
        );
    }
}

fn validate_format_field(
    context: &mut SchemaContext<'_, '_>,
    value: Option<&Value>,
    key_path: &str,
) {
    let Some(value) = value else {
        return;
    };
    let Some(raw) = value.as_str() else {
        context.unsupported_value(
            key_path,
            SchemaContext::value_type(value),
            "expected one of: text, markdown",
        );
        return;
    };
    if !matches!(raw, "text" | "markdown") {
        context.unsupported_value(key_path, raw, "expected one of: text, markdown");
    }
}

fn validate_out_field(context: &mut SchemaContext<'_, '_>, value: Option<&Value>, key_path: &str) {
    validate_optional_non_empty_string_field(context, value, key_path);
}

pub(super) fn field_path(section_path: &str, field: &str) -> String {
    format!("{section_path}.{field}")
}
