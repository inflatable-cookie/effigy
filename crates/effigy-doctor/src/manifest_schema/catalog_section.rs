use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::{require_table, validate_allowed_keys};
use super::values::validate_optional_non_empty_string_field;

/// Validate the explicit `[catalog]` membership section.
pub(super) fn validate_catalog_section(context: &mut SchemaContext<'_, '_>, value: &Value) {
    let Some(table) = require_table(context, "catalog", value, "expected table") else {
        return;
    };

    validate_allowed_keys(context, "catalog", table, &["alias", "members"]);

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
