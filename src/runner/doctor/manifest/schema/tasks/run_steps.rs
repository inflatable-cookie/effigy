use toml::Value;

use super::super::diagnostics::SchemaContext;
use super::super::tables::validate_allowed_keys;
use super::super::values::{
    validate_non_empty_string_or_array_of_non_empty_strings, validate_optional_boolean_field,
    validate_optional_integer_field, validate_string_array, validate_table_string_values,
};

pub(super) fn validate_run_step_table(
    context: &mut SchemaContext<'_, '_>,
    task_name: &str,
    index: usize,
    step_table: &toml::map::Map<String, Value>,
) {
    let step_path = format!("tasks.{task_name}.run[{index}]");
    validate_allowed_keys(
        context,
        &step_path,
        step_table,
        &[
            "run",
            "task",
            "env",
            "env_file",
            "id",
            "depends_on",
            "timeout_ms",
            "retry",
            "retry_delay_ms",
            "fail_fast",
        ],
    );

    if let Some(depends_on) = step_table.get("depends_on") {
        validate_depends_on(context, &step_path, depends_on);
    }
    validate_optional_integer_field(
        context,
        step_table.get("timeout_ms"),
        &format!("{step_path}.timeout_ms"),
    );
    validate_optional_integer_field(
        context,
        step_table.get("retry"),
        &format!("{step_path}.retry"),
    );
    validate_optional_integer_field(
        context,
        step_table.get("retry_delay_ms"),
        &format!("{step_path}.retry_delay_ms"),
    );
    validate_optional_boolean_field(
        context,
        step_table.get("fail_fast"),
        &format!("{step_path}.fail_fast"),
    );
    validate_env_table(context, &step_path, step_table.get("env"));
    validate_env_file(context, &step_path, step_table.get("env_file"));
}

fn validate_depends_on(context: &mut SchemaContext<'_, '_>, step_path: &str, depends_on: &Value) {
    validate_string_array(
        context,
        &format!("{step_path}.depends_on"),
        depends_on,
        "expected array of strings",
    );
}

fn validate_env_table(context: &mut SchemaContext<'_, '_>, step_path: &str, value: Option<&Value>) {
    let Some(value) = value else {
        return;
    };
    if let Some(profile_name) = value.as_str() {
        if profile_name.trim().is_empty() {
            context.unsupported_value(
                &format!("{step_path}.env"),
                "empty string",
                "expected non-empty profile name",
            );
        }
        return;
    }
    let Some(table) = value.as_table() else {
        context.unsupported_value(
            &format!("{step_path}.env"),
            SchemaContext::value_type(value),
            "expected table of string values or string profile name",
        );
        return;
    };
    validate_table_string_values(context, &format!("{step_path}.env"), table);
}

fn validate_env_file(context: &mut SchemaContext<'_, '_>, step_path: &str, value: Option<&Value>) {
    let Some(value) = value else {
        return;
    };
    validate_non_empty_string_or_array_of_non_empty_strings(
        context,
        &format!("{step_path}.env_file"),
        value,
        "expected string or array of strings",
    );
}
