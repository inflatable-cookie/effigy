use toml::Value;

use super::super::diagnostics::SchemaContext;
use super::super::tables::{require_table, validate_allowed_keys, validate_concurrent_array};

pub(super) fn validate_task_profiles(
    context: &mut SchemaContext<'_, '_>,
    task_name: &str,
    profiles: &Value,
) {
    let profiles_path = format!("tasks.{task_name}.profiles");
    let Some(profile_table) = require_table(context, &profiles_path, profiles, "expected table")
    else {
        return;
    };

    for (profile_name, profile_value) in profile_table {
        let profile_path = format!("{profiles_path}.{profile_name}");
        let Some(profile_inner) = require_table(
            context,
            &profile_path,
            profile_value,
            "expected table with `concurrent`",
        ) else {
            continue;
        };
        validate_allowed_keys(context, &profile_path, profile_inner, &["concurrent"]);
        if let Some(concurrent) = profile_inner.get("concurrent") {
            validate_concurrent_array(context, &format!("{profile_path}.concurrent"), concurrent);
        }
    }
}
