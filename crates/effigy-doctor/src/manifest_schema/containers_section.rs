use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::{require_table, validate_allowed_keys};

pub(super) fn validate_containers_section(context: &mut SchemaContext<'_, '_>, containers: &Value) {
    let Some(containers_table) = require_table(context, "containers", containers, "expected table")
    else {
        return;
    };
    if let Some(default) = containers_table.get("default") {
        if !default.is_str() {
            context.unsupported_value(
                "containers.default",
                SchemaContext::value_type(default),
                "expected string",
            );
        }
    }

    for (name, value) in containers_table {
        if name == "default" {
            continue;
        }
        let path = format!("containers.{name}");
        let Some(table) = require_table(context, &path, value, "expected table") else {
            continue;
        };
        validate_allowed_keys(
            context,
            &path,
            table,
            &[
                "driver",
                "startup",
                "profile",
                "compose_file",
                "project_name",
                "primary_service",
                "lifecycle",
                "health",
                "host",
                "ui",
            ],
        );
        validate_string_field(context, &path, table.get("driver"), "driver");
        validate_string_field(context, &path, table.get("startup"), "startup");
        validate_string_field(context, &path, table.get("profile"), "profile");
        validate_string_field(context, &path, table.get("compose_file"), "compose_file");
        validate_string_field(context, &path, table.get("project_name"), "project_name");
        validate_string_field(
            context,
            &path,
            table.get("primary_service"),
            "primary_service",
        );

        if let Some(lifecycle) = table.get("lifecycle") {
            validate_container_lifecycle(context, &path, lifecycle);
        }
        if let Some(health) = table.get("health") {
            validate_container_health(context, &path, health);
        }
        if let Some(host) = table.get("host") {
            validate_container_host(context, &path, host);
        }
        if let Some(ui) = table.get("ui") {
            validate_container_ui(context, &path, ui);
        }
    }
}

fn validate_container_lifecycle(
    context: &mut SchemaContext<'_, '_>,
    base_path: &str,
    value: &Value,
) {
    let path = format!("{base_path}.lifecycle");
    let Some(table) = require_table(context, &path, value, "expected table") else {
        return;
    };
    validate_allowed_keys(
        context,
        &path,
        table,
        &["on_task_exit", "shutdown", "detach_timeout_secs"],
    );
    validate_string_field(context, &path, table.get("on_task_exit"), "on_task_exit");
    validate_string_field(context, &path, table.get("shutdown"), "shutdown");
    if let Some(timeout) = table.get("detach_timeout_secs") {
        if timeout.as_integer().is_none() {
            context.unsupported_value(
                &format!("{path}.detach_timeout_secs"),
                SchemaContext::value_type(timeout),
                "expected integer",
            );
        }
    }
}

fn validate_container_health(context: &mut SchemaContext<'_, '_>, base_path: &str, value: &Value) {
    let path = format!("{base_path}.health");
    let Some(table) = require_table(context, &path, value, "expected table") else {
        return;
    };
    validate_allowed_keys(context, &path, table, &["check", "timeout_secs"]);
    validate_string_field(context, &path, table.get("check"), "check");
    if let Some(timeout) = table.get("timeout_secs") {
        if timeout.as_integer().is_none() {
            context.unsupported_value(
                &format!("{path}.timeout_secs"),
                SchemaContext::value_type(timeout),
                "expected integer",
            );
        }
    }
}

fn validate_container_host(context: &mut SchemaContext<'_, '_>, base_path: &str, value: &Value) {
    let path = format!("{base_path}.host");
    let Some(table) = require_table(context, &path, value, "expected table") else {
        return;
    };
    validate_allowed_keys(context, &path, table, &["ports", "mounts"]);
    validate_string_array(context, &format!("{path}.ports"), table.get("ports"));
    validate_string_array(context, &format!("{path}.mounts"), table.get("mounts"));
}

fn validate_container_ui(context: &mut SchemaContext<'_, '_>, base_path: &str, value: &Value) {
    let path = format!("{base_path}.ui");
    let Some(table) = require_table(context, &path, value, "expected table") else {
        return;
    };
    validate_allowed_keys(context, &path, table, &["tabs"]);
    validate_string_array(context, &format!("{path}.tabs"), table.get("tabs"));
}

fn validate_string_field(
    context: &mut SchemaContext<'_, '_>,
    path: &str,
    value: Option<&Value>,
    key: &str,
) {
    let Some(value) = value else {
        return;
    };
    if !value.is_str() {
        context.unsupported_value(
            &format!("{path}.{key}"),
            SchemaContext::value_type(value),
            "expected string",
        );
    }
}

fn validate_string_array(context: &mut SchemaContext<'_, '_>, path: &str, value: Option<&Value>) {
    let Some(value) = value else {
        return;
    };
    let Some(entries) = value.as_array() else {
        context.unsupported_value(path, SchemaContext::value_type(value), "expected array");
        return;
    };
    for (index, entry) in entries.iter().enumerate() {
        if !entry.is_str() {
            context.unsupported_value(
                &format!("{path}[{index}]"),
                SchemaContext::value_type(entry),
                "expected string",
            );
        }
    }
}
