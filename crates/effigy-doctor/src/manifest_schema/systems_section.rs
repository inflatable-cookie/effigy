use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::{require_table, validate_allowed_keys};
use super::values::validate_optional_non_empty_string_field;

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
            &["default_workspace", "workspaces"],
        );
        validate_optional_non_empty_string_field(
            context,
            system_table.get("default_workspace"),
            &format!("{system_path}.default_workspace"),
        );
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
        let Some(table) =
            require_table(context, &workspace_path, workspace_value, "expected table")
        else {
            continue;
        };
        validate_allowed_keys(context, &workspace_path, table, &["container", "workdir"]);
        validate_optional_non_empty_string_field(
            context,
            table.get("workdir"),
            &format!("{workspace_path}.workdir"),
        );
        if let Some(container) = table.get("container") {
            validate_workspace_container(
                context,
                &format!("{workspace_path}.container"),
                container,
            );
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
