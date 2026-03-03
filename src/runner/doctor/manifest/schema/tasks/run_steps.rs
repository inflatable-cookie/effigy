use toml::Value;

use super::super::diagnostics::SchemaContext;

pub(super) fn validate_run_step_table(
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
                | "env"
                | "env_file"
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
    validate_env_table(context, task_name, index, step_table.get("env"));
    validate_env_file(context, task_name, index, step_table.get("env_file"));
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

fn validate_env_table(
    context: &mut SchemaContext<'_, '_>,
    task_name: &str,
    index: usize,
    value: Option<&Value>,
) {
    let Some(value) = value else {
        return;
    };
    if let Some(profile_name) = value.as_str() {
        if profile_name.trim().is_empty() {
            context.unsupported_value(
                &format!("tasks.{task_name}.run[{index}].env"),
                "empty string",
                "expected non-empty profile name",
            );
        }
        return;
    }
    let Some(table) = value.as_table() else {
        context.unsupported_value(
            &format!("tasks.{task_name}.run[{index}].env"),
            SchemaContext::value_type(value),
            "expected table of string values or string profile name",
        );
        return;
    };
    for (key, entry) in table {
        if !entry.is_str() {
            context.unsupported_value(
                &format!("tasks.{task_name}.run[{index}].env.{key}"),
                SchemaContext::value_type(entry),
                "expected string",
            );
        }
    }
}

fn validate_env_file(
    context: &mut SchemaContext<'_, '_>,
    task_name: &str,
    index: usize,
    value: Option<&Value>,
) {
    let Some(value) = value else {
        return;
    };
    if let Some(raw) = value.as_str() {
        if raw.trim().is_empty() {
            context.unsupported_value(
                &format!("tasks.{task_name}.run[{index}].env_file"),
                "empty string",
                "expected non-empty string",
            );
        }
        return;
    }
    let Some(entries) = value.as_array() else {
        context.unsupported_value(
            &format!("tasks.{task_name}.run[{index}].env_file"),
            SchemaContext::value_type(value),
            "expected string or array of strings",
        );
        return;
    };
    if entries.is_empty() {
        context.unsupported_value(
            &format!("tasks.{task_name}.run[{index}].env_file"),
            "empty array",
            "expected non-empty array of strings",
        );
        return;
    }
    for (entry_index, entry) in entries.iter().enumerate() {
        let Some(raw) = entry.as_str() else {
            context.unsupported_value(
                &format!("tasks.{task_name}.run[{index}].env_file[{entry_index}]"),
                SchemaContext::value_type(entry),
                "expected string",
            );
            continue;
        };
        if raw.trim().is_empty() {
            context.unsupported_value(
                &format!("tasks.{task_name}.run[{index}].env_file[{entry_index}]"),
                "empty string",
                "expected non-empty string",
            );
        }
    }
}
