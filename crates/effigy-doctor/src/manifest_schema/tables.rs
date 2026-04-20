use toml::Value;

use super::diagnostics::SchemaContext;

pub(super) fn require_table<'a>(
    context: &mut SchemaContext<'_, '_>,
    path: &str,
    value: &'a Value,
    expected: &str,
) -> Option<&'a toml::map::Map<String, Value>> {
    let Some(table) = value.as_table() else {
        context.unsupported_value(path, SchemaContext::value_type(value), expected);
        return None;
    };
    Some(table)
}

pub(super) fn validate_allowed_keys(
    context: &mut SchemaContext<'_, '_>,
    base_path: &str,
    table: &toml::map::Map<String, Value>,
    allowed_keys: &[&str],
) {
    for key in table.keys() {
        if !allowed_keys.contains(&key.as_str()) {
            context.unsupported_key(&format!("{base_path}.{key}"));
        }
    }
}

pub(super) fn validate_known_table(
    context: &mut SchemaContext<'_, '_>,
    table_name: &str,
    value: &Value,
    allowed_keys: &[&str],
) {
    let Some(table) = require_table(context, table_name, value, "expected table") else {
        return;
    };
    validate_allowed_keys(context, table_name, table, allowed_keys);
}

pub(super) fn validate_named_string_or_known_table_entries(
    context: &mut SchemaContext<'_, '_>,
    base_path: &str,
    value: &Value,
    allowed_keys: &[&str],
    expected: &str,
) {
    let Some(table) = require_table(context, base_path, value, "expected a table") else {
        return;
    };

    for (name, entry_value) in table {
        let entry_path = format!("{base_path}.{name}");
        if entry_value.is_str() {
            continue;
        }
        let Some(inner) = entry_value.as_table() else {
            context.unsupported_value(
                &entry_path,
                SchemaContext::value_type(entry_value),
                expected,
            );
            continue;
        };
        validate_allowed_keys(context, &entry_path, inner, allowed_keys);
    }
}

pub(super) fn validate_concurrent_array(
    context: &mut SchemaContext<'_, '_>,
    path: &str,
    value: &Value,
) {
    let Some(entries) = value.as_array() else {
        context.unsupported_value(
            path,
            SchemaContext::value_type(value),
            "expected array of tables",
        );
        return;
    };

    for (index, entry) in entries.iter().enumerate() {
        let Some(table) = entry.as_table() else {
            context.unsupported_value(
                &format!("{path}[{index}]"),
                SchemaContext::value_type(entry),
                "expected table",
            );
            continue;
        };
        validate_allowed_keys(
            context,
            &format!("{path}[{index}]"),
            table,
            &[
                "name",
                "label",
                "role",
                "service",
                "task",
                "run",
                "start",
                "tab",
                "start_after_ms",
                "shutdown_on_exit",
            ],
        );
    }
}
