use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::{require_table, validate_allowed_keys};
use super::values::validate_optional_non_empty_string_field;

pub(super) fn validate_contracts_section(context: &mut SchemaContext<'_, '_>, contracts: &Value) {
    let Some(table) = require_table(
        context,
        "contracts",
        contracts,
        "expected table with optional keys: schema_index, selection_contract",
    ) else {
        return;
    };

    validate_allowed_keys(
        context,
        "contracts",
        table,
        &[
            "schema_index",
            "schema-index",
            "selection_contract",
            "selection-contract",
        ],
    );

    validate_optional_non_empty_string_field(
        context,
        table
            .get("schema_index")
            .or_else(|| table.get("schema-index")),
        "contracts.schema_index",
    );
    validate_optional_non_empty_string_field(
        context,
        table
            .get("selection_contract")
            .or_else(|| table.get("selection-contract")),
        "contracts.selection_contract",
    );
}
