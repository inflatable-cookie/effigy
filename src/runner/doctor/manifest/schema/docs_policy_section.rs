use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::{require_table, validate_allowed_keys};
use super::values::{
    validate_optional_non_empty_string_field, validate_optional_string_array_field,
};

pub(super) fn validate_docs_policy_section(
    context: &mut SchemaContext<'_, '_>,
    docs_policy: &Value,
) {
    let Some(docs_policy_table) = require_table(
        context,
        "docs_policy",
        docs_policy,
        "expected table with optional keys: indexes, next_actions",
    ) else {
        return;
    };

    validate_allowed_keys(
        context,
        "docs_policy",
        docs_policy_table,
        &["indexes", "next_actions", "next-actions"],
    );

    if let Some(indexes) = docs_policy_table.get("indexes") {
        validate_docs_policy_indexes(context, indexes);
    }
    if let Some(next_actions) = docs_policy_table
        .get("next_actions")
        .or_else(|| docs_policy_table.get("next-actions"))
    {
        validate_docs_policy_next_actions(context, next_actions);
    }
}

fn validate_docs_policy_indexes(context: &mut SchemaContext<'_, '_>, indexes: &Value) {
    let Some(indexes_table) = require_table(
        context,
        "docs_policy.indexes",
        indexes,
        "expected table of named index definitions",
    ) else {
        return;
    };

    for (name, entry_value) in indexes_table {
        let entry_path = format!("docs_policy.indexes.{name}");
        let Some(entry_table) = require_table(
            context,
            &entry_path,
            entry_value,
            "expected table with keys: file, dir, section, exclude",
        ) else {
            continue;
        };

        validate_allowed_keys(
            context,
            &entry_path,
            entry_table,
            &["file", "dir", "section", "exclude"],
        );
        validate_optional_non_empty_string_field(
            context,
            entry_table.get("file"),
            &format!("{entry_path}.file"),
        );
        validate_optional_non_empty_string_field(
            context,
            entry_table.get("dir"),
            &format!("{entry_path}.dir"),
        );
        validate_optional_non_empty_string_field(
            context,
            entry_table.get("section"),
            &format!("{entry_path}.section"),
        );
        validate_optional_string_array_field(
            context,
            entry_table.get("exclude"),
            &format!("{entry_path}.exclude"),
            "expected array of strings",
        );
    }
}

fn validate_docs_policy_next_actions(context: &mut SchemaContext<'_, '_>, next_actions: &Value) {
    let Some(next_actions_table) = require_table(
        context,
        "docs_policy.next_actions",
        next_actions,
        "expected table of named next-action definitions",
    ) else {
        return;
    };

    for (name, entry_value) in next_actions_table {
        let entry_path = format!("docs_policy.next_actions.{name}");
        let Some(entry_table) = require_table(
            context,
            &entry_path,
            entry_value,
            "expected table with keys: index, heading, allowlist_file",
        ) else {
            continue;
        };

        validate_allowed_keys(
            context,
            &entry_path,
            entry_table,
            &["index", "heading", "allowlist_file", "allowlist-file"],
        );
        validate_optional_non_empty_string_field(
            context,
            entry_table.get("index"),
            &format!("{entry_path}.index"),
        );
        validate_optional_non_empty_string_field(
            context,
            entry_table.get("heading"),
            &format!("{entry_path}.heading"),
        );
        validate_optional_non_empty_string_field(
            context,
            entry_table
                .get("allowlist_file")
                .or_else(|| entry_table.get("allowlist-file")),
            &format!("{entry_path}.allowlist_file"),
        );
    }
}
