use toml::Value;

use super::diagnostics::SchemaContext;

/// Validate the current `[bootstrap]` declaration.
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
        let accepted = setup
            .as_array()
            .is_some_and(|entries| entries.iter().all(Value::is_str));
        if !accepted {
            context.invalid_value_type(
                "bootstrap.setup",
                "an ordered list of task selector strings",
            );
        }
    }
    if let Some(start) = table.get("start") {
        let accepted = start.is_str();
        if !accepted {
            context.invalid_value_type(
                "bootstrap.start",
                "one task selector string; sequence steps inside a task",
            );
        }
    }
    if let Some(submodules) = table.get("submodules") {
        let accepted = submodules.is_bool();
        if !accepted {
            context.invalid_value_type("bootstrap.submodules", "boolean");
        }
    }
}
