use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::{require_table, validate_allowed_keys};
use super::values::{validate_optional_boolean_field, validate_optional_string_array_field};

/// Validate the `[catalog]` section, including the nested
/// `[catalog.discovery]` table that controls ambient catalog discovery.
///
/// `discovery` is consumed by `effigy-routing` discovery (`enabled` toggles
/// ambient child discovery; `ignore` lists directory names to skip). It must
/// be recognized here so a repo that legitimately configures it does not trip
/// `manifest.schema.unsupported_key`.
pub(super) fn validate_catalog_section(context: &mut SchemaContext<'_, '_>, value: &Value) {
    let Some(table) = require_table(context, "catalog", value, "expected table") else {
        return;
    };

    validate_allowed_keys(context, "catalog", table, &["alias", "discovery"]);

    if let Some(alias) = table.get("alias") {
        if !alias.is_str() {
            context.unsupported_value(
                "catalog.alias",
                SchemaContext::value_type(alias),
                "expected string",
            );
        }
    }

    if let Some(discovery) = table.get("discovery") {
        let Some(discovery_table) =
            require_table(context, "catalog.discovery", discovery, "expected table")
        else {
            return;
        };
        validate_allowed_keys(
            context,
            "catalog.discovery",
            discovery_table,
            &["enabled", "ignore"],
        );
        validate_optional_boolean_field(
            context,
            discovery_table.get("enabled"),
            "catalog.discovery.enabled",
        );
        validate_optional_string_array_field(
            context,
            discovery_table.get("ignore"),
            "catalog.discovery.ignore",
            "expected array of strings",
        );
    }
}
