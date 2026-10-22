use toml::Value;

use super::diagnostics::SchemaContext;

pub(super) fn validate_bootstrap_section(context: &mut SchemaContext<'_, '_>, value: &Value) {
    let Some(table) = value.as_table() else {
        context.unsupported_value("bootstrap", "wrong value type", "table");
        return;
    };

    for key in table.keys() {
        if !matches!(key.as_str(), "setup" | "start" | "submodules") {
            context.unsupported_nested_key("bootstrap", key);
        }
    }

    if let Some(setup) = table.get("setup") {
        match setup.as_array() {
            Some(entries) => {
                if entries
                    .iter()
                    .any(|entry| !matches!(entry, Value::String(_)))
                {
                    context.invalid_value_type(
                        "bootstrap.setup[]",
                        "task selector strings; move inline steps into a task",
                    );
                }
            }
            None => context.invalid_value_type(
                "bootstrap.setup",
                "an ordered list of task selector strings",
            ),
        }
    }
    if let Some(start) = table.get("start") {
        if !matches!(start, Value::String(_)) {
            context.invalid_value_type(
                "bootstrap.start",
                "one task selector string; sequence steps inside a task",
            );
        }
    }
    if let Some(submodules) = table.get("submodules") {
        if !matches!(submodules, Value::Boolean(_)) {
            context.invalid_value_type("bootstrap.submodules", "true or omitted");
        }
    }
}
