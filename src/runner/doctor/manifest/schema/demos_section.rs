use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::{require_table, validate_allowed_keys};
use super::values::{
    validate_optional_enum_string_field, validate_optional_non_empty_string_field,
    validate_optional_string_array_field,
};

pub(super) fn validate_demos_section(context: &mut SchemaContext<'_, '_>, value: &Value) {
    let Some(table) = require_table(context, "demos", value, "expected table") else {
        return;
    };

    for (demo_id, entry_value) in table {
        let entry_path = format!("demos.{demo_id}");
        let Some(entry) = require_table(context, &entry_path, entry_value, "expected table") else {
            continue;
        };
        validate_allowed_keys(
            context,
            &entry_path,
            entry,
            &[
                "title",
                "summary",
                "proof",
                "owner",
                "mode",
                "status",
                "covers",
                "tags",
                "artifacts",
                "receipt",
                "task",
                "run",
                "prerequisites",
                "dependencies",
                "depends_on",
            ],
        );

        validate_optional_non_empty_string_field(
            context,
            entry.get("title"),
            &format!("{entry_path}.title"),
        );
        validate_optional_non_empty_string_field(
            context,
            entry.get("summary"),
            &format!("{entry_path}.summary"),
        );
        validate_optional_non_empty_string_field(
            context,
            entry.get("proof"),
            &format!("{entry_path}.proof"),
        );
        validate_optional_non_empty_string_field(
            context,
            entry.get("owner"),
            &format!("{entry_path}.owner"),
        );
        validate_optional_enum_string_field(
            context,
            entry.get("mode"),
            &format!("{entry_path}.mode"),
            &["headless", "interactive", "hybrid"],
            "expected one of: headless, interactive, hybrid",
        );
        validate_optional_enum_string_field(
            context,
            entry.get("status"),
            &format!("{entry_path}.status"),
            &[
                "planned", "ready", "running", "passed", "failed", "broken", "missing",
            ],
            "expected one of: planned, ready, running, passed, failed, broken, missing",
        );
        validate_optional_string_array_field(
            context,
            entry.get("covers"),
            &format!("{entry_path}.covers"),
            "expected array of strings",
        );
        validate_optional_string_array_field(
            context,
            entry.get("tags"),
            &format!("{entry_path}.tags"),
            "expected array of strings",
        );
        validate_optional_string_array_field(
            context,
            entry.get("artifacts"),
            &format!("{entry_path}.artifacts"),
            "expected array of strings",
        );
        validate_optional_non_empty_string_field(
            context,
            entry.get("receipt"),
            &format!("{entry_path}.receipt"),
        );
        validate_optional_non_empty_string_field(
            context,
            entry.get("task"),
            &format!("{entry_path}.task"),
        );
        validate_optional_non_empty_string_field(
            context,
            entry.get("run"),
            &format!("{entry_path}.run"),
        );
        validate_optional_string_array_field(
            context,
            entry.get("prerequisites"),
            &format!("{entry_path}.prerequisites"),
            "expected array of strings",
        );
        let dependencies_value = entry
            .get("dependencies")
            .or_else(|| entry.get("depends_on"));
        validate_optional_string_array_field(
            context,
            dependencies_value,
            &format!("{entry_path}.dependencies"),
            "expected array of strings",
        );

        let has_task = entry.get("task").is_some();
        let has_run = entry.get("run").is_some();
        if has_task == has_run {
            context.unsupported_value(
                &entry_path,
                "missing or ambiguous entrypoint",
                "expected exactly one of `task` or `run`",
            );
        }
    }
}
