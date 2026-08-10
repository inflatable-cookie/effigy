use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::{require_table, validate_allowed_keys};
use super::values::{validate_optional_boolean_field, validate_optional_non_empty_string_field};

pub(super) fn validate_systems_section(context: &mut SchemaContext<'_, '_>, systems: &Value) {
    let Some(table) = require_table(context, "systems", systems, "expected table") else {
        return;
    };

    validate_optional_non_empty_string_field(context, table.get("default"), "systems.default");

    for (system_name, system_value) in table {
        if system_name == "default" {
            continue;
        }
        let system_path = format!("systems.{system_name}");
        let Some(system_table) =
            require_table(context, &system_path, system_value, "expected table")
        else {
            continue;
        };
        validate_allowed_keys(
            context,
            &system_path,
            system_table,
            &[
                "default_workspace",
                "workspaces",
                "container",
                "working_dir",
                "user",
                "home",
                "mounts",
            ],
        );
        validate_optional_non_empty_string_field(
            context,
            system_table.get("default_workspace"),
            &format!("{system_path}.default_workspace"),
        );
        validate_workspace_fields(context, &system_path, system_table);
        if let Some(workspaces) = system_table.get("workspaces") {
            validate_workspaces_table(context, &format!("{system_path}.workspaces"), workspaces);
        }
    }
}

fn validate_workspaces_table(context: &mut SchemaContext<'_, '_>, path: &str, workspaces: &Value) {
    let Some(workspace_table) = require_table(context, path, workspaces, "expected table") else {
        return;
    };

    for (workspace_name, workspace_value) in workspace_table {
        let workspace_path = format!("{path}.{workspace_name}");
        validate_workspace_entry(context, &workspace_path, workspace_value);
    }
}

fn validate_workspace_entry(context: &mut SchemaContext<'_, '_>, path: &str, value: &Value) {
    let Some(table) = require_table(context, path, value, "expected table") else {
        return;
    };
    validate_allowed_keys(
        context,
        path,
        table,
        &["container", "working_dir", "user", "home", "mounts"],
    );
    validate_workspace_fields(context, path, table);
}

fn validate_workspace_fields(
    context: &mut SchemaContext<'_, '_>,
    path: &str,
    table: &toml::map::Map<String, Value>,
) {
    validate_optional_non_empty_string_field(
        context,
        table.get("working_dir"),
        &format!("{path}.working_dir"),
    );
    validate_optional_non_empty_string_field(context, table.get("user"), &format!("{path}.user"));
    validate_optional_non_empty_string_field(context, table.get("home"), &format!("{path}.home"));
    if let Some(mounts) = table.get("mounts") {
        validate_system_mounts(context, &format!("{path}.mounts"), mounts);
    }
    if let Some(container) = table.get("container") {
        validate_workspace_container(context, &format!("{path}.container"), container);
    }
}

fn validate_system_mounts(context: &mut SchemaContext<'_, '_>, path: &str, mounts: &Value) {
    let Some(entries) = mounts.as_array() else {
        context.unsupported_value(path, SchemaContext::value_type(mounts), "expected array");
        return;
    };
    for (index, mount) in entries.iter().enumerate() {
        let mount_path = format!("{path}[{index}]");
        if mount.is_str() {
            validate_optional_non_empty_string_field(context, Some(mount), &mount_path);
            continue;
        }
        let Some(table) = require_table(
            context,
            &mount_path,
            mount,
            "expected string or structured mount table",
        ) else {
            continue;
        };
        validate_allowed_keys(
            context,
            &mount_path,
            table,
            &["member", "source", "target", "options", "catalog"],
        );
        validate_optional_non_empty_string_field(
            context,
            table.get("member"),
            &format!("{mount_path}.member"),
        );
        validate_optional_non_empty_string_field(
            context,
            table.get("source"),
            &format!("{mount_path}.source"),
        );
        validate_optional_non_empty_string_field(
            context,
            table.get("target"),
            &format!("{mount_path}.target"),
        );
        validate_optional_boolean_field(
            context,
            table.get("catalog"),
            &format!("{mount_path}.catalog"),
        );
        let has_member = table.contains_key("member");
        let has_source = table.contains_key("source");
        if has_member == has_source {
            context.unsupported_value(
                &mount_path,
                "invalid source declaration",
                "exactly one of `member` or `source`",
            );
        }
        if has_member && table.contains_key("catalog") {
            context.unsupported_value(
                &format!("{mount_path}.catalog"),
                "catalog flag on member mount",
                "omit `catalog`; member mounts imply catalog membership",
            );
        }
        if let Some(options) = table.get("options") {
            let Some(options) = options.as_array() else {
                context.unsupported_value(
                    &format!("{mount_path}.options"),
                    SchemaContext::value_type(options),
                    "expected array of non-empty strings",
                );
                continue;
            };
            for (option_index, option) in options.iter().enumerate() {
                validate_optional_non_empty_string_field(
                    context,
                    Some(option),
                    &format!("{mount_path}.options[{option_index}]"),
                );
            }
        }
    }
}

fn validate_workspace_container(
    context: &mut SchemaContext<'_, '_>,
    path: &str,
    container: &Value,
) {
    if container.is_str() {
        validate_optional_non_empty_string_field(context, Some(container), path);
        return;
    }
    let Some(table) = require_table(context, path, container, "expected string or table") else {
        return;
    };
    validate_allowed_keys(context, path, table, &["image", "mount"]);
    validate_optional_non_empty_string_field(context, table.get("image"), &format!("{path}.image"));
    validate_optional_non_empty_string_field(context, table.get("mount"), &format!("{path}.mount"));
}
