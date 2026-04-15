use toml::Value;

use super::diagnostics::SchemaContext;

pub(super) fn validate_bootstrap_section(context: &mut SchemaContext<'_, '_>, value: &Value) {
    let Some(table) = value.as_table() else {
        context.unsupported_value("bootstrap", "wrong value type", "table");
        return;
    };

    for key in table.keys() {
        if !matches!(key.as_str(), "setup" | "start" | "submodules" | "children") {
            context.unsupported_nested_key("bootstrap", key);
        }
    }

    if let Some(children) = table.get("children") {
        let Some(array) = children.as_array() else {
            context.invalid_value_type("bootstrap.children", "array");
            return;
        };
        for child in array {
            let Some(child_table) = child.as_table() else {
                context.invalid_value_type("bootstrap.children[]", "table");
                continue;
            };
            for key in child_table.keys() {
                if !matches!(
                    key.as_str(),
                    "path" | "repo" | "branch" | "setup" | "required"
                ) {
                    context.unsupported_nested_key("bootstrap.children[]", key);
                }
            }
        }
    }
}
