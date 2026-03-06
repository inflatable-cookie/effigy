use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::{validate_allowed_keys, validate_named_string_or_known_table_entries};
use super::values::validate_optional_enum_string_field;

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
    validate_named_string_or_known_table_entries(
        context,
        "test.suites",
        suites,
        &["run"],
        "expected string command or table with `run`",
    );
}
