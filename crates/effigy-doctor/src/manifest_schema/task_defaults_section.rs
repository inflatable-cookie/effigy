use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::{require_table, validate_allowed_keys};
use super::values::validate_optional_enum_string_field;

pub(super) fn validate_task_defaults_section(context: &mut SchemaContext<'_, '_>, value: &Value) {
    let Some(table) = require_table(
        context,
        "task_defaults",
        value,
        "expected table with optional keys: run_in",
    ) else {
        return;
    };

    validate_allowed_keys(context, "task_defaults", table, &["run_in"]);
    validate_optional_enum_string_field(
        context,
        table.get("run_in"),
        "task_defaults.run_in",
        &["host", "container", "either"],
        "expected `host`, `container`, or `either`",
    );
}
