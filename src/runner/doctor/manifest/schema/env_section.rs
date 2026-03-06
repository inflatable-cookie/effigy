use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::require_table;
use super::values::validate_table_string_values;

pub(super) fn validate_env_section(context: &mut SchemaContext<'_, '_>, env: &Value) {
    let Some(table) = require_table(context, "env", env, "expected table") else {
        return;
    };

    for (profile, profile_value) in table {
        if profile_value.is_str() {
            continue;
        }
        let Some(entries) = profile_value.as_array() else {
            context.unsupported_value(
                &format!("env.{profile}"),
                SchemaContext::value_type(profile_value),
                "expected string value or array of tables",
            );
            continue;
        };
        for (index, entry) in entries.iter().enumerate() {
            let entry_path = format!("env.{profile}[{index}]");
            let Some(env_table) = require_table(context, &entry_path, entry, "expected table")
            else {
                continue;
            };
            validate_table_string_values(context, &entry_path, env_table);
        }
    }
}
