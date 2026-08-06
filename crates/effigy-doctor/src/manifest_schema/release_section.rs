use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::{require_table, validate_allowed_keys};
use super::values::{
    validate_optional_boolean_field, validate_optional_non_empty_string_field,
    validate_optional_string_array_field,
};

pub(super) fn validate_release_section(context: &mut SchemaContext<'_, '_>, release: &Value) {
    let Some(release_table) = require_table(
        context,
        "release",
        release,
        "expected table with optional keys: version_file, version_path, changelog, pre-1-0, initial-tag-current-version, sync_files, gates, tag_format",
    ) else {
        return;
    };

    validate_allowed_keys(
        context,
        "release",
        release_table,
        &[
            "version_file",
            "version-file",
            "version_path",
            "version-path",
            "changelog",
            "pre-1-0",
            "initial_tag_current_version",
            "initial-tag-current-version",
            "sync_files",
            "sync-files",
            "gates",
            "tag_format",
            "tag-format",
        ],
    );

    validate_optional_non_empty_string_field(
        context,
        release_table
            .get("version_file")
            .or_else(|| release_table.get("version-file")),
        "release.version_file",
    );
    validate_optional_non_empty_string_field(
        context,
        release_table
            .get("version_path")
            .or_else(|| release_table.get("version-path")),
        "release.version_path",
    );
    validate_optional_non_empty_string_field(
        context,
        release_table.get("changelog"),
        "release.changelog",
    );
    validate_optional_boolean_field(context, release_table.get("pre-1-0"), "release.pre-1-0");
    validate_optional_boolean_field(
        context,
        release_table
            .get("initial_tag_current_version")
            .or_else(|| release_table.get("initial-tag-current-version")),
        "release.initial_tag_current_version",
    );
    validate_optional_string_array_field(
        context,
        release_table
            .get("sync_files")
            .or_else(|| release_table.get("sync-files")),
        "release.sync_files",
        "expected array of strings",
    );
    validate_optional_non_empty_string_field(
        context,
        release_table
            .get("tag_format")
            .or_else(|| release_table.get("tag-format")),
        "release.tag_format",
    );

    if let Some(gates) = release_table.get("gates") {
        validate_release_gates(context, gates);
    }
}

fn validate_release_gates(context: &mut SchemaContext<'_, '_>, gates: &Value) {
    let Some(gates_table) = require_table(
        context,
        "release.gates",
        gates,
        "expected table of named gate commands",
    ) else {
        return;
    };

    for (name, gate_value) in gates_table {
        let gate_path = format!("release.gates.{name}");
        if gate_value.is_str() {
            validate_optional_non_empty_string_field(context, Some(gate_value), &gate_path);
            continue;
        }

        let Some(gate_table) = require_table(
            context,
            &gate_path,
            gate_value,
            "expected string command or table with `command`/`description`",
        ) else {
            continue;
        };

        validate_allowed_keys(context, &gate_path, gate_table, &["command", "description"]);
        validate_optional_non_empty_string_field(
            context,
            gate_table.get("command"),
            &format!("{gate_path}.command"),
        );
        validate_optional_non_empty_string_field(
            context,
            gate_table.get("description"),
            &format!("{gate_path}.description"),
        );
    }
}
