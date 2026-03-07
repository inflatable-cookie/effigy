use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::{
    require_table, validate_allowed_keys, validate_named_string_or_known_table_entries,
};
use super::values::{
    validate_optional_boolean_field, validate_optional_enum_string_field,
    validate_optional_integer_field, validate_optional_non_empty_string_field,
    validate_optional_non_empty_string_or_array_field,
    validate_optional_non_empty_string_or_table_string_values_field,
    validate_optional_string_array_field,
};

pub(super) fn validate_test_section(context: &mut SchemaContext<'_, '_>, test: &Value) {
    let Some(test_table) = test.as_table() else {
        context.unsupported_value(
            "test",
            SchemaContext::value_type(test),
            "expected table with optional keys: max_parallel, cargo_env_match, runners, suites",
        );
        return;
    };
    validate_allowed_keys(
        context,
        "test",
        test_table,
        &["max_parallel", "cargo_env_match", "runners", "suites"],
    );
    if let Some(cargo_env_match) = test_table.get("cargo_env_match") {
        validate_test_cargo_env_match(context, cargo_env_match);
    }
    if let Some(runners) = test_table.get("runners") {
        validate_test_runners(context, runners);
    }
    if let Some(suites) = test_table.get("suites") {
        validate_test_suites(context, suites);
    }
}

fn validate_test_cargo_env_match(context: &mut SchemaContext<'_, '_>, value: &Value) {
    validate_optional_enum_string_field(
        context,
        Some(value),
        "test.cargo_env_match",
        &["executable-only", "prefix-aware", "shell-aware"],
        "expected one of: executable-only, prefix-aware, shell-aware",
    );
}

fn validate_test_runners(context: &mut SchemaContext<'_, '_>, runners: &Value) {
    validate_named_string_or_known_table_entries(
        context,
        "test.runners",
        runners,
        &["command"],
        "expected string command or table with `command`",
    );
}

fn validate_test_suites(context: &mut SchemaContext<'_, '_>, suites: &Value) {
    let Some(suites_table) = require_table(context, "test.suites", suites, "expected a table")
    else {
        return;
    };

    for (suite_name, entry_value) in suites_table {
        let suite_path = format!("test.suites.{suite_name}");
        if entry_value.is_str() {
            continue;
        }
        let Some(suite_table) = entry_value.as_table() else {
            context.unsupported_value(
                &suite_path,
                SchemaContext::value_type(entry_value),
                "expected string command or table with `run`",
            );
            continue;
        };
        validate_allowed_keys(
            context,
            &suite_path,
            suite_table,
            &[
                "run",
                "env",
                "env_file",
                "setup",
                "teardown",
                "teardown_policy",
            ],
        );
        validate_optional_non_empty_string_field(
            context,
            suite_table.get("run"),
            &format!("{suite_path}.run"),
        );
        validate_optional_non_empty_string_or_table_string_values_field(
            context,
            suite_table.get("env"),
            &format!("{suite_path}.env"),
            "expected table of string values or string profile name",
        );
        validate_optional_non_empty_string_or_array_field(
            context,
            suite_table.get("env_file"),
            &format!("{suite_path}.env_file"),
            "expected string or array of strings",
        );
        validate_optional_enum_string_field(
            context,
            suite_table.get("teardown_policy"),
            &format!("{suite_path}.teardown_policy"),
            &["always", "on-success"],
            "expected one of: always, on-success",
        );
        validate_test_suite_run_steps(context, &suite_path, "setup", suite_table.get("setup"));
        validate_test_suite_run_steps(
            context,
            &suite_path,
            "teardown",
            suite_table.get("teardown"),
        );
    }
}

fn validate_test_suite_run_steps(
    context: &mut SchemaContext<'_, '_>,
    suite_path: &str,
    field_name: &str,
    value: Option<&Value>,
) {
    let Some(value) = value else {
        return;
    };
    let path = format!("{suite_path}.{field_name}");
    let Some(steps) = value.as_array() else {
        context.unsupported_value(
            &path,
            SchemaContext::value_type(value),
            "expected array of strings or tables",
        );
        return;
    };

    for (index, step) in steps.iter().enumerate() {
        let step_path = format!("{path}[{index}]");
        if step.is_str() {
            continue;
        }
        let Some(step_table) = step.as_table() else {
            context.unsupported_value(
                &step_path,
                SchemaContext::value_type(step),
                "expected string command or table with `run`/`task`",
            );
            continue;
        };
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
            validate_optional_string_array_field(
                context,
                Some(depends_on),
                &format!("{step_path}.depends_on"),
                "expected array of strings",
            );
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
            step_table.get("run"),
            &format!("{step_path}.run"),
        );
        validate_optional_non_empty_string_field(
            context,
            step_table.get("task"),
            &format!("{step_path}.task"),
        );
        validate_optional_non_empty_string_field(
            context,
            step_table.get("id"),
            &format!("{step_path}.id"),
        );
        validate_optional_non_empty_string_or_table_string_values_field(
            context,
            step_table.get("env"),
            &format!("{step_path}.env"),
            "expected table of string values or string profile name",
        );
        validate_optional_non_empty_string_or_array_field(
            context,
            step_table.get("env_file"),
            &format!("{step_path}.env_file"),
            "expected string or array of strings",
        );
    }
}
