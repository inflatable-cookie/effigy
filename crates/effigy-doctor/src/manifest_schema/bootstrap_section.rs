use toml::Value;

use super::diagnostics::SchemaContext;

/// `[bootstrap]` keys are validated against the v0.15 shape, plus the v0.14.1
/// spellings that still load for one release.
///
/// A v0.14.1 project must keep loading after an update (contract 052 §11).
/// `run`, `[bootstrap].children`, `{ task = ... }` entries, multi-selector
/// `start` chains and the `submodules` policy strings are read by the manifest
/// translation and reported as one deprecation notice instead of a schema
/// finding; `effigy bootstrap` alone refuses the retired-only ones.
pub(super) fn validate_bootstrap_section(context: &mut SchemaContext<'_, '_>, value: &Value) {
    let Some(table) = value.as_table() else {
        context.unsupported_value("bootstrap", "wrong value type", "table");
        return;
    };

    for key in table.keys() {
        if !matches!(
            key.as_str(),
            "setup" | "start" | "submodules" | "run" | "children"
        ) {
            context.unsupported_nested_key("bootstrap", key);
        }
    }

    if let Some(setup) = table.get("setup") {
        // A v0.15.1 selector list, or the v0.14.1 `run` alias shape that the
        // manifest translation reads (a single definition or an entry list).
        let accepted = matches!(setup, Value::Array(_) | Value::String(_) | Value::Table(_));
        if !accepted {
            context.invalid_value_type(
                "bootstrap.setup",
                "an ordered list of task selector strings",
            );
        }
    }
    if let Some(start) = table.get("start") {
        let accepted = matches!(start, Value::String(_) | Value::Array(_) | Value::Table(_));
        if !accepted {
            context.invalid_value_type(
                "bootstrap.start",
                "one task selector string; sequence steps inside a task",
            );
        }
    }
    if let Some(submodules) = table.get("submodules") {
        let accepted = matches!(submodules, Value::Boolean(_) | Value::String(_));
        if !accepted {
            context.invalid_value_type("bootstrap.submodules", "true or omitted");
        }
    }
}
