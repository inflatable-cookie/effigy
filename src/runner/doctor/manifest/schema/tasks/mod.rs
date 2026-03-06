use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::{require_table, validate_allowed_keys, validate_concurrent_array};
use super::values::{
    validate_optional_enum_string_field, validate_optional_non_empty_string_or_array_field,
    validate_optional_table_string_values_field,
};

mod profiles;
mod run_steps;

use profiles::validate_task_profiles;
use run_steps::validate_run_step_table;

pub(super) fn validate_tasks_table(context: &mut SchemaContext<'_, '_>, tasks: &Value) {
    let Some(tasks_table) = require_table(
        context,
        "tasks",
        tasks,
        "expected a table of task definitions",
    ) else {
        return;
    };

    for (task_name, task_value) in tasks_table {
        if task_value.is_str() || task_value.is_array() {
            validate_compact_task_value(context, task_name, task_value);
            continue;
        }

        let task_path = format!("tasks.{task_name}");
        let Some(task_table) = require_table(
            context,
            &task_path,
            task_value,
            "expected string command, run sequence array, or task table",
        ) else {
            continue;
        };

        validate_task_table_keys(context, task_name, task_table);
        validate_task_mode(context, task_name, task_table.get("mode"));
        validate_task_run_field(context, task_name, task_table.get("run"));
        validate_task_env_field(context, task_name, task_table.get("env"));
        validate_task_env_file_field(context, task_name, task_table.get("env_file"));

        if let Some(concurrent) = task_table.get("concurrent") {
            validate_concurrent_array(
                context,
                &format!("tasks.{task_name}.concurrent"),
                concurrent,
            );
        }
        if let Some(profiles) = task_table.get("profiles") {
            validate_task_profiles(context, task_name, profiles);
        }
    }
}

fn validate_compact_task_value(
    context: &mut SchemaContext<'_, '_>,
    task_name: &str,
    task_value: &Value,
) {
    let Some(array) = task_value.as_array() else {
        return;
    };

    for (index, step) in array.iter().enumerate() {
        if let Some(step_table) = step.as_table() {
            validate_run_step_table(context, task_name, index, step_table);
        } else if !step.is_str() {
            context.unsupported_value(
                &format!("tasks.{task_name}.run[{index}]"),
                SchemaContext::value_type(step),
                "expected string command or table with `run`/`task`",
            );
        }
    }
}

fn validate_task_table_keys(
    context: &mut SchemaContext<'_, '_>,
    task_name: &str,
    task_table: &toml::map::Map<String, Value>,
) {
    validate_allowed_keys(
        context,
        &format!("tasks.{task_name}"),
        task_table,
        &[
            "run",
            "env",
            "env_file",
            "mode",
            "fail_on_non_zero",
            "concurrent",
            "profiles",
        ],
    );
}

fn validate_task_mode(context: &mut SchemaContext<'_, '_>, task_name: &str, mode: Option<&Value>) {
    validate_optional_enum_string_field(
        context,
        mode,
        &format!("tasks.{task_name}.mode"),
        &["tui"],
        "expected `tui`",
    );
}

fn validate_task_run_field(
    context: &mut SchemaContext<'_, '_>,
    task_name: &str,
    run: Option<&Value>,
) {
    let Some(run) = run else {
        return;
    };
    if !(run.is_str() || run.is_array()) {
        context.unsupported_value(
            &format!("tasks.{task_name}.run"),
            SchemaContext::value_type(run),
            "expected string command or run-step array",
        );
    }
}

fn validate_task_env_field(
    context: &mut SchemaContext<'_, '_>,
    task_name: &str,
    env: Option<&Value>,
) {
    validate_optional_table_string_values_field(
        context,
        env,
        &format!("tasks.{task_name}.env"),
        "expected table of string values",
    );
}

fn validate_task_env_file_field(
    context: &mut SchemaContext<'_, '_>,
    task_name: &str,
    env_file: Option<&Value>,
) {
    validate_optional_non_empty_string_or_array_field(
        context,
        env_file,
        &format!("tasks.{task_name}.env_file"),
        "expected string or array of strings",
    );
}
