use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::{require_table, validate_allowed_keys};

pub(super) fn validate_manifest_section(context: &mut SchemaContext<'_, '_>, value: &Value) {
    let Some(table) = require_table(context, "manifest", value, "expected table") else {
        return;
    };
    validate_allowed_keys(
        context,
        "manifest",
        table,
        &["include", "minimum_effigy_version"],
    );

    if let Some(minimum) = table.get("minimum_effigy_version") {
        if !minimum.is_str() {
            context.unsupported_value(
                "manifest.minimum_effigy_version",
                SchemaContext::value_type(minimum),
                "expected string",
            );
        }
    }

    let Some(include) = table.get("include") else {
        return;
    };
    let Some(entries) = include.as_array() else {
        context.unsupported_value(
            "manifest.include",
            SchemaContext::value_type(include),
            "expected array of strings or tables",
        );
        return;
    };

    for (index, entry) in entries.iter().enumerate() {
        let entry_path = format!("manifest.include[{index}]");
        if entry.is_str() {
            continue;
        }
        let Some(entry_table) = entry.as_table() else {
            context.unsupported_value(
                &entry_path,
                SchemaContext::value_type(entry),
                "expected string or table",
            );
            continue;
        };
        validate_allowed_keys(context, &entry_path, entry_table, &["path", "override"]);
        if let Some(override_value) = entry_table.get("override") {
            let Some(override_entries) = override_value.as_array() else {
                context.unsupported_value(
                    &format!("{entry_path}.override"),
                    SchemaContext::value_type(override_value),
                    "expected array of strings",
                );
                continue;
            };
            for (override_index, override_entry) in override_entries.iter().enumerate() {
                if !override_entry.is_str() {
                    context.unsupported_value(
                        &format!("{entry_path}.override[{override_index}]"),
                        SchemaContext::value_type(override_entry),
                        "expected string",
                    );
                }
            }
        }
    }
}
