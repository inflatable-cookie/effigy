use toml::Value;

use super::super::diagnostics::SchemaContext;
use super::super::tables::validate_allowed_keys;
use super::super::values::{
    validate_optional_boolean_field, validate_optional_integer_field,
    validate_optional_non_empty_string_field,
    validate_optional_non_empty_string_or_array_field,
    validate_optional_non_empty_string_or_table_string_values_field,
    validate_optional_string_array_field,
};

pub(in crate::runner::doctor::manifest::schema) fn validate_run_step_table(
    context: &mut SchemaContext<'_, '_>,
    owner_path: &str,
    index: usize,
    step_table: &toml::map::Map<String, Value>,
) {
    let step_path = format!("{owner_path}.run[{index}]");
    validate_allowed_keys(
        context,
        &step_path,
        step_table,
        &[
            "run",
            "task",
            "rhai",
            "rhai_file",
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
    validate_optional_non_empty_string_field(
        context,
        step_table.get("rhai"),
        &format!("{step_path}.rhai"),
    );
    validate_optional_non_empty_string_field(
        context,
        step_table.get("rhai_file"),
        &format!("{step_path}.rhai_file"),
    );
    validate_env_table(context, &step_path, step_table.get("env"));
    validate_env_file(context, &step_path, step_table.get("env_file"));
}

fn validate_depends_on(context: &mut SchemaContext<'_, '_>, step_path: &str, depends_on: &Value) {
    validate_optional_string_array_field(
        context,
        Some(depends_on),
        &format!("{step_path}.depends_on"),
        "expected array of strings",
    );
}

fn validate_env_table(context: &mut SchemaContext<'_, '_>, step_path: &str, value: Option<&Value>) {
    validate_optional_non_empty_string_or_table_string_values_field(
        context,
        value,
        &format!("{step_path}.env"),
        "expected table of string values or string profile name",
    );
}

fn validate_env_file(context: &mut SchemaContext<'_, '_>, step_path: &str, value: Option<&Value>) {
    validate_optional_non_empty_string_or_array_field(
        context,
        value,
        &format!("{step_path}.env_file"),
        "expected string or array of strings",
    );
}
