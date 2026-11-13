use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::{require_table, validate_allowed_keys};
use super::values::{
    validate_optional_boolean_field, validate_optional_integer_field,
    validate_optional_string_array_field, validate_optional_string_field,
    validate_optional_string_fields,
};

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
                "services",
                "working_dir",
                "aliases",
                "dns",
                "project_name",
                "primary_service",
                "data_from",
                "lifecycle",
                "health",
                "host",
                "host_processes",
                "ui",
            ],
        );
        validate_optional_string_fields(
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
                "data_from",
                "working_dir",
            ],
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
        if let Some(services) = table.get("services") {
            validate_container_services(context, &path, services);
        }
        if let Some(aliases) = table.get("aliases") {
            validate_container_aliases(context, &path, aliases);
        }
        if let Some(dns) = table.get("dns") {
            validate_container_dns(context, &path, dns);
        }
        if let Some(host_processes) = table.get("host_processes") {
            validate_container_host_processes(context, &path, host_processes);
        }
    }
}

fn validate_container_host_processes(
    context: &mut SchemaContext<'_, '_>,
    base_path: &str,
    value: &Value,
) {
    let path = format!("{base_path}.host_processes");
    let Some(entries) = value.as_array() else {
        context.unsupported_value(
            &path,
            SchemaContext::value_type(value),
            "expected array of tables",
        );
        return;
    };
    for (index, entry) in entries.iter().enumerate() {
        let entry_path = format!("{path}[{index}]");
        let Some(table) = entry.as_table() else {
            context.unsupported_value(
                &entry_path,
                SchemaContext::value_type(entry),
                "expected table",
            );
            continue;
        };
        validate_allowed_keys(
            context,
            &entry_path,
            table,
            &[
                "name",
                "run",
                "cwd",
                "env",
                "depends_on",
                "listener",
                "restart",
                "restart_delay_ms",
                "shutdown_signal",
                "shutdown_grace_secs",
            ],
        );
        validate_optional_string_fields(context, &entry_path, table, &["name", "run", "cwd"]);
        if let Some(env) = table.get("env") {
            validate_host_process_env(context, &format!("{entry_path}.env"), env);
        }
        validate_optional_string_array_field(
            context,
            table.get("depends_on"),
            &format!("{entry_path}.depends_on"),
            "expected array",
        );
        if let Some(listener) = table.get("listener") {
            validate_host_process_listener(context, &format!("{entry_path}.listener"), listener);
        }
        validate_optional_string_fields(
            context,
            &entry_path,
            table,
            &["restart", "shutdown_signal"],
        );
        validate_optional_integer_field(
            context,
            table.get("restart_delay_ms"),
            &format!("{entry_path}.restart_delay_ms"),
        );
        validate_optional_integer_field(
            context,
            table.get("shutdown_grace_secs"),
            &format!("{entry_path}.shutdown_grace_secs"),
        );
    }
}

/// A host process's `env` is literal text plus the resolved-reference syntax
/// `${service.<name>.host}`, `${service.<name>.port}`,
/// `${route.<domain>.public_url}`, `${host_process.<name>.public_url}`,
/// `${host_process.<name>.internal_url}`, `${domains}`, `${apex}` and
/// `${gateway.https_port}` (effigy#321). Names are validated when the container
/// policy loads; this schema check only requires string values.
fn validate_host_process_env(context: &mut SchemaContext<'_, '_>, path: &str, value: &Value) {
    let Some(env) = require_table(context, path, value, "expected table of strings") else {
        return;
    };
    for (key, entry) in env {
        if !entry.is_str() {
            context.unsupported_value(
                &format!("{path}.{key}"),
                SchemaContext::value_type(entry),
                "expected string",
            );
        }
    }
}

/// The managed listener contract: a loopback bind, a readiness probe and a
/// gateway route.
fn validate_host_process_listener(context: &mut SchemaContext<'_, '_>, path: &str, value: &Value) {
    let Some(listener) = require_table(context, path, value, "expected table") else {
        return;
    };
    validate_allowed_keys(context, path, listener, &["bind", "readiness", "route"]);
    validate_optional_string_field(context, listener.get("bind"), &format!("{path}.bind"));
    if let Some(readiness) = listener.get("readiness") {
        let readiness_path = format!("{path}.readiness");
        if let Some(table) = require_table(context, &readiness_path, readiness, "expected table") {
            validate_allowed_keys(
                context,
                &readiness_path,
                table,
                &["path", "status", "timeout_secs"],
            );
            validate_optional_string_field(
                context,
                table.get("path"),
                &format!("{readiness_path}.path"),
            );
            validate_optional_integer_field(
                context,
                table.get("status"),
                &format!("{readiness_path}.status"),
            );
            validate_optional_integer_field(
                context,
                table.get("timeout_secs"),
                &format!("{readiness_path}.timeout_secs"),
            );
        }
    }
    if let Some(route) = listener.get("route") {
        let route_path = format!("{path}.route");
        if let Some(table) = require_table(context, &route_path, route, "expected table") {
            validate_allowed_keys(context, &route_path, table, &["domain", "tls"]);
            validate_optional_string_field(
                context,
                table.get("domain"),
                &format!("{route_path}.domain"),
            );
            validate_optional_boolean_field(
                context,
                table.get("tls"),
                &format!("{route_path}.tls"),
            );
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
        validate_optional_string_fields(
            context,
            &alias_path,
            alias_fields,
            &["service", "command"],
        );
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
        validate_optional_string_fields(
            context,
            &service_path,
            table,
            &["catalog", "variant", "config"],
        );
        validate_optional_boolean_field(
            context,
            table.get("shared"),
            &format!("{service_path}.shared"),
        );
    }
}

fn validate_container_dns(context: &mut SchemaContext<'_, '_>, base_path: &str, value: &Value) {
    let path = format!("{base_path}.dns");
    let Some(table) = require_table(context, &path, value, "expected table") else {
        return;
    };
    validate_allowed_keys(
        context,
        &path,
        table,
        &["base_domain", "routes", "domains", "domain_defaults"],
    );
    validate_optional_string_field(
        context,
        table.get("base_domain"),
        &format!("{path}.base_domain"),
    );
    validate_optional_string_array_field(
        context,
        table.get("domains"),
        &format!("{path}.domains"),
        "expected array of strings",
    );
    if let Some(defaults) = table.get("domain_defaults") {
        match defaults.as_table() {
            Some(defaults_table) => {
                let defaults_path = format!("{path}.domain_defaults");
                validate_allowed_keys(
                    context,
                    &defaults_path,
                    defaults_table,
                    &["tls", "port", "service", "target_host"],
                );
                validate_optional_boolean_field(
                    context,
                    defaults_table.get("tls"),
                    &format!("{defaults_path}.tls"),
                );
                validate_optional_integer_field(
                    context,
                    defaults_table.get("port"),
                    &format!("{defaults_path}.port"),
                );
                validate_optional_string_fields(
                    context,
                    &defaults_path,
                    defaults_table,
                    &["service", "target_host"],
                );
            }
            None => context.unsupported_value(
                &format!("{path}.domain_defaults"),
                SchemaContext::value_type(defaults),
                "expected table",
            ),
        }
    }
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
                &["domain", "tls", "port", "service", "target_host"],
            );
            validate_optional_string_fields(
                context,
                &route_path,
                route_table,
                &["domain", "service", "target_host"],
            );
            validate_optional_boolean_field(
                context,
                route_table.get("tls"),
                &format!("{route_path}.tls"),
            );
            validate_optional_integer_field(
                context,
                route_table.get("port"),
                &format!("{route_path}.port"),
            );
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
    validate_optional_string_fields(context, &path, table, &["on_task_exit", "shutdown"]);
    validate_optional_integer_field(
        context,
        table.get("detach_timeout_secs"),
        &format!("{path}.detach_timeout_secs"),
    );
}

fn validate_container_health(context: &mut SchemaContext<'_, '_>, base_path: &str, value: &Value) {
    let path = format!("{base_path}.health");
    let Some(table) = require_table(context, &path, value, "expected table") else {
        return;
    };
    validate_allowed_keys(context, &path, table, &["check", "timeout_secs"]);
    validate_optional_string_field(context, table.get("check"), &format!("{path}.check"));
    validate_optional_integer_field(
        context,
        table.get("timeout_secs"),
        &format!("{path}.timeout_secs"),
    );
}

fn validate_container_host(context: &mut SchemaContext<'_, '_>, base_path: &str, value: &Value) {
    let path = format!("{base_path}.host");
    let Some(table) = require_table(context, &path, value, "expected table") else {
        return;
    };
    validate_allowed_keys(context, &path, table, &["ports", "mounts"]);
    validate_optional_string_array_field(
        context,
        table.get("ports"),
        &format!("{path}.ports"),
        "expected array",
    );
    validate_container_host_mounts(context, &path, table.get("mounts"));
}

fn validate_container_host_mounts(
    context: &mut SchemaContext<'_, '_>,
    base_path: &str,
    value: Option<&Value>,
) {
    let path = format!("{base_path}.mounts");
    let Some(value) = value else {
        return;
    };
    let Some(entries) = value.as_array() else {
        context.unsupported_value(
            &path,
            SchemaContext::value_type(value),
            "expected array of strings or tables",
        );
        return;
    };
    for (index, entry) in entries.iter().enumerate() {
        let entry_path = format!("{path}[{index}]");
        if entry.is_str() {
            continue;
        }
        let Some(table) = entry.as_table() else {
            context.unsupported_value(
                &entry_path,
                SchemaContext::value_type(entry),
                "expected string or table",
            );
            continue;
        };
        validate_allowed_keys(
            context,
            &entry_path,
            table,
            &["host", "container", "external", "options"],
        );
        validate_optional_string_fields(context, &entry_path, table, &["host", "container"]);
        validate_optional_boolean_field(
            context,
            table.get("external"),
            &format!("{entry_path}.external"),
        );
        validate_optional_string_array_field(
            context,
            table.get("options"),
            &format!("{entry_path}.options"),
            "expected array",
        );
    }
}

fn validate_container_ui(context: &mut SchemaContext<'_, '_>, base_path: &str, value: &Value) {
    let path = format!("{base_path}.ui");
    let Some(table) = require_table(context, &path, value, "expected table") else {
        return;
    };
    validate_allowed_keys(context, &path, table, &["tabs"]);
    validate_optional_string_array_field(
        context,
        table.get("tabs"),
        &format!("{path}.tabs"),
        "expected array",
    );
}
