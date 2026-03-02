use toml::Value;

use super::diagnostics::SchemaContext;

pub(super) fn validate_known_table(
    context: &mut SchemaContext<'_, '_>,
    table_name: &str,
    value: &Value,
    allowed_keys: &[&str],
) {
    let Some(table) = value.as_table() else {
        context.unsupported_value(
            table_name,
            SchemaContext::value_type(value),
            "expected table",
        );
        return;
    };
    for key in table.keys() {
        if !allowed_keys.contains(&key.as_str()) {
            context.unsupported_key(&format!("{table_name}.{key}"));
        }
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
        for key in table.keys() {
            if !matches!(
                key.as_str(),
                "name" | "task" | "run" | "start" | "tab" | "start_after_ms"
            ) {
                context.unsupported_key(&format!("{path}[{index}].{key}"));
            }
        }
    }
}
