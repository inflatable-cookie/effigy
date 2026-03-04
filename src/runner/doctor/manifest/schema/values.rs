use toml::Value;

use super::diagnostics::SchemaContext;

pub(super) fn validate_optional_integer_field(
    context: &mut SchemaContext<'_, '_>,
    value: Option<&Value>,
    path: &str,
) {
    let Some(value) = value else {
        return;
    };
    if !value.is_integer() {
        context.unsupported_value(path, SchemaContext::value_type(value), "expected integer");
    }
}

pub(super) fn validate_optional_boolean_field(
    context: &mut SchemaContext<'_, '_>,
    value: Option<&Value>,
    path: &str,
) {
    let Some(value) = value else {
        return;
    };
    if !value.is_bool() {
        context.unsupported_value(path, SchemaContext::value_type(value), "expected boolean");
    }
}

pub(super) fn validate_string_array(
    context: &mut SchemaContext<'_, '_>,
    path: &str,
    value: &Value,
    expected: &str,
) {
    let Some(entries) = value.as_array() else {
        context.unsupported_value(path, SchemaContext::value_type(value), expected);
        return;
    };

    for (index, entry) in entries.iter().enumerate() {
        if !entry.is_str() {
            context.unsupported_value(
                &format!("{path}[{index}]"),
                SchemaContext::value_type(entry),
                "expected string",
            );
        }
    }
}

pub(super) fn validate_table_string_values(
    context: &mut SchemaContext<'_, '_>,
    base_path: &str,
    table: &toml::map::Map<String, Value>,
) {
    for (key, value) in table {
        if !value.is_str() {
            context.unsupported_value(
                &format!("{base_path}.{key}"),
                SchemaContext::value_type(value),
                "expected string",
            );
        }
    }
}

pub(super) fn validate_non_empty_string_or_array_of_non_empty_strings(
    context: &mut SchemaContext<'_, '_>,
    path: &str,
    value: &Value,
    expected: &str,
) {
    if let Some(raw) = value.as_str() {
        if raw.trim().is_empty() {
            context.unsupported_value(path, "empty string", "expected non-empty string");
        }
        return;
    }

    let Some(entries) = value.as_array() else {
        context.unsupported_value(path, SchemaContext::value_type(value), expected);
        return;
    };
    if entries.is_empty() {
        context.unsupported_value(path, "empty array", "expected non-empty array of strings");
        return;
    }

    for (index, entry) in entries.iter().enumerate() {
        let Some(raw) = entry.as_str() else {
            context.unsupported_value(
                &format!("{path}[{index}]"),
                SchemaContext::value_type(entry),
                "expected string",
            );
            continue;
        };
        if raw.trim().is_empty() {
            context.unsupported_value(
                &format!("{path}[{index}]"),
                "empty string",
                "expected non-empty string",
            );
        }
    }
}
