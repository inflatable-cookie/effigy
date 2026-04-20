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
                "context",
                "profile",
                "compose_file",
                "services",
                "working_dir",
                "aliases",
                "dns",
                "project_name",
                "primary_service",
                "lifecycle",
                "health",
                "host",
                "workspace",
                "ui",
            ],
        );
        validate_string_field(context, &path, table.get("driver"), "driver");
        validate_string_field(context, &path, table.get("startup"), "startup");
        validate_string_field(context, &path, table.get("context"), "context");
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
        if let Some(workspace) = table.get("workspace") {
            validate_container_workspace(context, &path, workspace);
        }
        if let Some(ui) = table.get("ui") {
            validate_container_ui(context, &path, ui);
        }
        if let Some(services) = table.get("services") {
            validate_container_services(context, &path, services);
        }
        validate_string_field(context, &path, table.get("working_dir"), "working_dir");
        if let Some(aliases) = table.get("aliases") {
            validate_container_aliases(context, &path, aliases);
        }
        if let Some(dns) = table.get("dns") {
            validate_container_dns(context, &path, dns);
        }
    }
}

fn validate_container_aliases(context: &mut SchemaContext<'_, '_>, base_path: &str, value: &Value) {
    let aliases_path = format!("{base_path}.aliases");
    let Some(alias_table) = require_table(context, &aliases_path, value, "expected table") else {
        return;
    };
    for (alias_name, alias_value) in alias_table {
        let alias_path = format!("{aliases_path}.{alias_name}");
        if let Some(service) = alias_value.as_str() {
            if service.trim().is_empty() {
                context.unsupported_value(&alias_path, "string", "expected non-empty service name");
            }
            continue;
        }
        let Some(alias_fields) = require_table(
            context,
            &alias_path,
            alias_value,
            "expected string or table",
        ) else {
            continue;
        };
        validate_allowed_keys(context, &alias_path, alias_fields, &["service", "command"]);
        validate_string_field(context, &alias_path, alias_fields.get("service"), "service");
        validate_string_field(context, &alias_path, alias_fields.get("command"), "command");
    }
}

fn validate_container_services(
    context: &mut SchemaContext<'_, '_>,
    base_path: &str,
    value: &Value,
) {
    let path = format!("{base_path}.services");
    let Some(services) = require_table(context, &path, value, "expected table") else {
        return;
    };

    for (service_name, service_value) in services {
        let service_path = format!("{path}.{service_name}");
        let Some(table) = require_table(context, &service_path, service_value, "expected table")
        else {
            continue;
        };
        if let Some(catalog) = table.get("catalog") {
            validate_string_field(context, &service_path, Some(catalog), "catalog");
        }
        if let Some(variant) = table.get("variant") {
            validate_string_field(context, &service_path, Some(variant), "variant");
        }
        if let Some(config) = table.get("config") {
            validate_string_field(context, &service_path, Some(config), "config");
        }
        if let Some(shared) = table.get("shared") {
            if shared.as_bool().is_none() {
                context.unsupported_value(
                    &format!("{service_path}.shared"),
                    SchemaContext::value_type(shared),
                    "expected boolean",
                );
            }
        }
    }
}

fn validate_container_dns(context: &mut SchemaContext<'_, '_>, base_path: &str, value: &Value) {
    let path = format!("{base_path}.dns");
    let Some(table) = require_table(context, &path, value, "expected table") else {
        return;
    };
    validate_allowed_keys(context, &path, table, &["routes"]);
    if let Some(routes) = table.get("routes") {
        let Some(entries) = routes.as_array() else {
            context.unsupported_value(
                &format!("{path}.routes"),
                SchemaContext::value_type(routes),
                "expected array of tables",
            );
            return;
        };
        for (index, entry) in entries.iter().enumerate() {
            let route_path = format!("{path}.routes[{index}]");
            let Some(route_table) = entry.as_table() else {
                context.unsupported_value(
                    &route_path,
                    SchemaContext::value_type(entry),
                    "expected table",
                );
                continue;
            };
            validate_allowed_keys(
                context,
                &route_path,
                route_table,
                &["domain", "tls", "port", "service"],
            );
            validate_string_field(context, &route_path, route_table.get("domain"), "domain");
            validate_string_field(context, &route_path, route_table.get("service"), "service");
            if let Some(tls) = route_table.get("tls") {
                if tls.as_bool().is_none() {
                    context.unsupported_value(
                        &format!("{route_path}.tls"),
                        SchemaContext::value_type(tls),
                        "expected boolean",
                    );
                }
            }
            if let Some(port) = route_table.get("port") {
                if port.as_integer().is_none() {
                    context.unsupported_value(
                        &format!("{route_path}.port"),
                        SchemaContext::value_type(port),
                        "expected integer",
                    );
                }
            }
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

fn validate_container_workspace(
    context: &mut SchemaContext<'_, '_>,
    base_path: &str,
    value: &Value,
) {
    let path = format!("{base_path}.workspace");
    let Some(table) = require_table(context, &path, value, "expected table") else {
        return;
    };
    validate_allowed_keys(context, &path, table, &["extra_mounts", "user", "home"]);
    validate_string_array(
        context,
        &format!("{path}.extra_mounts"),
        table.get("extra_mounts"),
    );
    validate_string_field(context, &path, table.get("user"), "user");
    validate_string_field(context, &path, table.get("home"), "home");
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
