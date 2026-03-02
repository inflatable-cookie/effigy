use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::validate_known_table;

pub(super) fn validate_package_manager_section(
    context: &mut SchemaContext<'_, '_>,
    package_manager: &Value,
) {
    validate_known_table(
        context,
        "package_manager",
        package_manager,
        &["js", "js_ts", "typescript"],
    );
    let Some(pm_table) = package_manager.as_table() else {
        return;
    };
    for alias in ["js", "js_ts", "typescript"] {
        let Some(value) = pm_table.get(alias) else {
            continue;
        };
        if let Some(raw) = value.as_str() {
            if !matches!(raw, "bun" | "pnpm" | "npm" | "direct") {
                context.unsupported_value(
                    "package_manager.js",
                    raw,
                    "expected one of: bun, pnpm, npm, direct",
                );
            }
        } else {
            context.unsupported_value(
                "package_manager.js",
                SchemaContext::value_type(value),
                "expected a string value",
            );
        }
    }
}
