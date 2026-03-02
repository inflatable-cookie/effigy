use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::validate_concurrent_array;

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

fn validate_run_step_table(
    context: &mut SchemaContext<'_, '_>,
    task_name: &str,
    index: usize,
    step_table: &toml::map::Map<String, Value>,
) {
    for key in step_table.keys() {
        if !matches!(
            key.as_str(),
            "run"
                | "task"
                | "id"
                | "depends_on"
                | "timeout_ms"
                | "retry"
                | "retry_delay_ms"
                | "fail_fast"
        ) {
            context.unsupported_key(&format!("tasks.{task_name}.run[{index}].{key}"));
        }
    }

    if let Some(depends_on) = step_table.get("depends_on") {
        validate_depends_on(context, task_name, index, depends_on);
    }
    validate_integer_field(
        context,
        task_name,
        index,
        step_table.get("timeout_ms"),
        "timeout_ms",
    );
    validate_integer_field(context, task_name, index, step_table.get("retry"), "retry");
    validate_integer_field(
        context,
        task_name,
        index,
        step_table.get("retry_delay_ms"),
        "retry_delay_ms",
    );
    validate_boolean_field(
        context,
        task_name,
        index,
        step_table.get("fail_fast"),
        "fail_fast",
    );
}

fn validate_depends_on(
    context: &mut SchemaContext<'_, '_>,
    task_name: &str,
    index: usize,
    depends_on: &Value,
) {
    let Some(deps) = depends_on.as_array() else {
        context.unsupported_value(
            &format!("tasks.{task_name}.run[{index}].depends_on"),
            SchemaContext::value_type(depends_on),
            "expected array of strings",
        );
        return;
    };

    for (dep_index, dep) in deps.iter().enumerate() {
        if !dep.is_str() {
            context.unsupported_value(
                &format!("tasks.{task_name}.run[{index}].depends_on[{dep_index}]"),
                SchemaContext::value_type(dep),
                "expected string",
            );
        }
    }
}

fn validate_integer_field(
    context: &mut SchemaContext<'_, '_>,
    task_name: &str,
    index: usize,
    value: Option<&Value>,
    field: &str,
) {
    let Some(value) = value else {
        return;
    };
    if !value.is_integer() {
        context.unsupported_value(
            &format!("tasks.{task_name}.run[{index}].{field}"),
            SchemaContext::value_type(value),
            "expected integer",
        );
    }
}

fn validate_boolean_field(
    context: &mut SchemaContext<'_, '_>,
    task_name: &str,
    index: usize,
    value: Option<&Value>,
    field: &str,
) {
    let Some(value) = value else {
        return;
    };
    if !value.is_bool() {
        context.unsupported_value(
            &format!("tasks.{task_name}.run[{index}].{field}"),
            SchemaContext::value_type(value),
            "expected boolean",
        );
    }
}

fn validate_task_table_keys(
    context: &mut SchemaContext<'_, '_>,
    task_name: &str,
    task_table: &toml::map::Map<String, Value>,
) {
    for key in task_table.keys() {
        if !matches!(
            key.as_str(),
            "run" | "mode" | "fail_on_non_zero" | "shell" | "concurrent" | "profiles"
        ) {
            context.unsupported_key(&format!("tasks.{task_name}.{key}"));
        }
    }
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

fn validate_task_profiles(context: &mut SchemaContext<'_, '_>, task_name: &str, profiles: &Value) {
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
