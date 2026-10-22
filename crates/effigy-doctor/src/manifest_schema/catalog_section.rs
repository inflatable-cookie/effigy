use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::{require_table, validate_allowed_keys};
use super::values::validate_optional_non_empty_string_field;

/// Validate the explicit `[catalog]` membership section.
pub(super) fn validate_catalog_section(context: &mut SchemaContext<'_, '_>, value: &Value) {
    let Some(table) = require_table(context, "catalog", value, "expected table") else {
        return;
    };

    // `graph` was removed with the code graph and keeps loading for one
    // release (effigy#274); the manifest names it in its deprecation notice.
    validate_allowed_keys(context, "catalog", table, &["alias", "members", "graph"]);

    if let Some(alias) = table.get("alias") {
        if !alias.is_str() {
            context.unsupported_value(
                "catalog.alias",
                SchemaContext::value_type(alias),
                "expected string",
            );
        }
    }

    if let Some(members) = table.get("members") {
        if let Some(members) = require_table(context, "catalog.members", members, "expected table")
        {
            for (member, value) in members {
                validate_optional_non_empty_string_field(
                    context,
                    Some(value),
                    &format!("catalog.members.{member}"),
                );
            }
        }
    }
}
