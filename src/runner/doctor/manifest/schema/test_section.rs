use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::validate_allowed_keys;

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
    let Some(raw) = value.as_str() else {
        context.unsupported_value(
            "test.cargo_env_match",
            SchemaContext::value_type(value),
            "expected one of: executable-only, prefix-aware, shell-aware",
        );
        return;
    };

    if !matches!(raw, "executable-only" | "prefix-aware" | "shell-aware") {
        context.unsupported_value(
            "test.cargo_env_match",
            SchemaContext::value_type(value),
            "expected one of: executable-only, prefix-aware, shell-aware",
        );
    }
}

fn validate_test_runners(context: &mut SchemaContext<'_, '_>, runners: &Value) {
    let Some(runners_table) = runners.as_table() else {
        context.unsupported_value(
            "test.runners",
            SchemaContext::value_type(runners),
            "expected a table",
        );
        return;
    };

    for (runner_name, runner_value) in runners_table {
        if let Some(inner) = runner_value.as_table() {
            validate_allowed_keys(
                context,
                &format!("test.runners.{runner_name}"),
                inner,
                &["command"],
            );
        } else if !runner_value.is_str() {
            context.unsupported_value(
                &format!("test.runners.{runner_name}"),
                SchemaContext::value_type(runner_value),
                "expected string command or table with `command`",
            );
        }
    }
}

fn validate_test_suites(context: &mut SchemaContext<'_, '_>, suites: &Value) {
    let Some(suites_table) = suites.as_table() else {
        context.unsupported_value(
            "test.suites",
            SchemaContext::value_type(suites),
            "expected a table",
        );
        return;
    };

    for (suite_name, suite_value) in suites_table {
        if let Some(inner) = suite_value.as_table() {
            validate_allowed_keys(
                context,
                &format!("test.suites.{suite_name}"),
                inner,
                &["run"],
            );
        } else if !suite_value.is_str() {
            context.unsupported_value(
                &format!("test.suites.{suite_name}"),
                SchemaContext::value_type(suite_value),
                "expected string command or table with `run`",
            );
        }
    }
}
