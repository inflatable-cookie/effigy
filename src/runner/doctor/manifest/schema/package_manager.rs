use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::validate_known_table;
use super::values::validate_optional_enum_string_field;

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
        validate_optional_enum_string_field(
            context,
            pm_table.get(alias),
            &format!("package_manager.{alias}"),
            &["bun", "pnpm", "npm", "direct"],
            "expected one of: bun, pnpm, npm, direct",
        );
    }
}
