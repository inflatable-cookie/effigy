use toml::Value;

use super::diagnostics::SchemaContext;

pub(super) fn validate_top_level_keys(
    context: &mut SchemaContext<'_, '_>,
    table: &toml::map::Map<String, Value>,
) {
    let allowed_top = [
        "catalog",
        "defer",
        "env",
        "test",
        "package_manager",
        "scan",
        "shell",
        "tasks",
    ];
    for key in table.keys() {
        if !allowed_top.contains(&key.as_str()) {
            context.unsupported_key(key);
        }
    }
}
