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
