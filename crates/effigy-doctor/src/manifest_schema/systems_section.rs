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
        super::values::validate_string_array(
            context,
            &format!("{path}.mounts"),
            mounts,
            "expected array of strings",
        );
    }
    if let Some(container) = table.get("container") {
        validate_workspace_container(context, &format!("{path}.container"), container);
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
