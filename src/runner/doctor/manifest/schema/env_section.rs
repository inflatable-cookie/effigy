use toml::Value;

use super::diagnostics::SchemaContext;
use super::values::validate_table_string_values;

pub(super) fn validate_env_section(context: &mut SchemaContext<'_, '_>, env: &Value) {
    let Some(table) = env.as_table() else {
        context.unsupported_value("env", SchemaContext::value_type(env), "expected table");
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
            let Some(env_table) = entry.as_table() else {
                context.unsupported_value(
                    &format!("env.{profile}[{index}]"),
                    SchemaContext::value_type(entry),
                    "expected table",
                );
                continue;
            };
            validate_table_string_values(context, &format!("env.{profile}[{index}]"), env_table);
        }
    }
}
