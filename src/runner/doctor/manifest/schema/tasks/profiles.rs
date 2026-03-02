use toml::Value;

use super::super::diagnostics::SchemaContext;
use super::super::tables::validate_concurrent_array;

pub(super) fn validate_task_profiles(
    context: &mut SchemaContext<'_, '_>,
    task_name: &str,
    profiles: &Value,
) {
    let Some(profile_table) = profiles.as_table() else {
        context.unsupported_value(
            &format!("tasks.{task_name}.profiles"),
            SchemaContext::value_type(profiles),
            "expected table",
        );
        return;
    };

    for (profile_name, profile_value) in profile_table {
        let Some(profile_inner) = profile_value.as_table() else {
            context.unsupported_value(
                &format!("tasks.{task_name}.profiles.{profile_name}"),
                SchemaContext::value_type(profile_value),
                "expected table with `concurrent`",
            );
            continue;
        };
        for key in profile_inner.keys() {
            if key != "concurrent" {
                context
                    .unsupported_key(&format!("tasks.{task_name}.profiles.{profile_name}.{key}"));
            }
        }
        if let Some(concurrent) = profile_inner.get("concurrent") {
            validate_concurrent_array(
                context,
                &format!("tasks.{task_name}.profiles.{profile_name}.concurrent"),
                concurrent,
            );
        }
    }
}
