use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::{validate_allowed_keys, validate_concurrent_array};
use super::values::{
    validate_non_empty_string_or_array_of_non_empty_strings, validate_table_string_values,
};

mod profiles;
mod run_steps;

use profiles::validate_task_profiles;
use run_steps::validate_run_step_table;

pub(super) fn validate_tasks_table(context: &mut SchemaContext<'_, '_>, tasks: &Value) {
    let Some(tasks_table) = tasks.as_table() else {
        context.unsupported_value(
            "tasks",
            SchemaContext::value_type(tasks),
            "expected a table of task definitions",
        );
        return;
    };

    for (task_name, task_value) in tasks_table {
        if task_value.is_str() || task_value.is_array() {
            validate_compact_task_value(context, task_name, task_value);
            continue;
        }

        let Some(task_table) = task_value.as_table() else {
            context.unsupported_value(
                &format!("tasks.{task_name}"),
                SchemaContext::value_type(task_value),
                "expected string command, run sequence array, or task table",
            );
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
            "shell",
            "concurrent",
            "profiles",
        ],
    );
}

fn validate_task_mode(context: &mut SchemaContext<'_, '_>, task_name: &str, mode: Option<&Value>) {
    let Some(mode) = mode else {
        return;
    };
    if let Some(raw) = mode.as_str() {
        if raw != "tui" {
            context.unsupported_value(&format!("tasks.{task_name}.mode"), raw, "expected `tui`");
        }
    } else {
        context.unsupported_value(
            &format!("tasks.{task_name}.mode"),
            SchemaContext::value_type(mode),
            "expected string `tui`",
        );
    }
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
    let Some(env) = env else {
        return;
    };
    let Some(env_table) = env.as_table() else {
        context.unsupported_value(
            &format!("tasks.{task_name}.env"),
            SchemaContext::value_type(env),
            "expected table of string values",
        );
        return;
    };
    validate_table_string_values(context, &format!("tasks.{task_name}.env"), env_table);
}

fn validate_task_env_file_field(
    context: &mut SchemaContext<'_, '_>,
    task_name: &str,
    env_file: Option<&Value>,
) {
    let Some(env_file) = env_file else {
        return;
    };
    validate_non_empty_string_or_array_of_non_empty_strings(
        context,
        &format!("tasks.{task_name}.env_file"),
        env_file,
        "expected string or array of strings",
    );
}
