use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::{require_table, validate_allowed_keys};

pub(super) fn validate_distribution_section(
    context: &mut SchemaContext<'_, '_>,
    distribution: &Value,
) {
    let Some(distribution_table) =
        require_table(context, "distribution", distribution, "expected table")
    else {
        return;
    };
    validate_allowed_keys(
        context,
        "distribution",
        distribution_table,
        &["package", "publish", "preflight", "metadata", "closeout"],
    );

    if let Some(package) = distribution_table.get("package") {
        validate_known_string_table(
            context,
            "distribution.package",
            package,
            &["name", "repo-url", "brew-formula"],
        );
    }
    if let Some(publish) = distribution_table.get("publish") {
        let Some(publish_table) =
            require_table(context, "distribution.publish", publish, "expected table")
        else {
            return;
        };
        validate_allowed_keys(
            context,
            "distribution.publish",
            publish_table,
            &[
                "binary-name",
                "registry-label",
                "verify-tag-install",
                "verify-binary-json-tasks",
            ],
        );
        for key in ["binary-name", "registry-label"] {
            let Some(value) = publish_table.get(key) else {
                continue;
            };
            if !value.is_str() {
                context.unsupported_value(
                    &format!("distribution.publish.{key}"),
                    SchemaContext::value_type(value),
                    "expected string",
                );
            }
        }
        for key in ["verify-tag-install", "verify-binary-json-tasks"] {
            let Some(value) = publish_table.get(key) else {
                continue;
            };
            if !value.is_bool() {
                context.unsupported_value(
                    &format!("distribution.publish.{key}"),
                    SchemaContext::value_type(value),
                    "expected boolean",
                );
            }
        }
    }
    if let Some(preflight) = distribution_table.get("preflight") {
        validate_known_string_table(
            context,
            "distribution.preflight",
            preflight,
            &["docs-task", "smoke-task"],
        );
    }
    if let Some(metadata) = distribution_table.get("metadata") {
        let Some(metadata_table) =
            require_table(context, "distribution.metadata", metadata, "expected table")
        else {
            return;
        };
        validate_allowed_keys(
            context,
            "distribution.metadata",
            metadata_table,
            &["required-docs", "required-files"],
        );

        for (field, value) in [
            (
                "distribution.metadata.required-docs",
                metadata_table.get("required-docs"),
            ),
            (
                "distribution.metadata.required-files",
                metadata_table.get("required-files"),
            ),
        ] {
            let Some(value) = value else {
                continue;
            };
            let Some(entries) = value.as_array() else {
                context.unsupported_value(
                    field,
                    SchemaContext::value_type(value),
                    "expected array",
                );
                continue;
            };
            for (index, entry) in entries.iter().enumerate() {
                if !entry.is_str() {
                    context.unsupported_value(
                        &format!("{field}[{index}]"),
                        SchemaContext::value_type(entry),
                        "expected string",
                    );
                }
            }
        }
    }
    if let Some(closeout) = distribution_table.get("closeout") {
        validate_known_string_table(
            context,
            "distribution.closeout",
            closeout,
            &["owner", "related", "next-step"],
        );
    }
}

fn validate_known_string_table(
    context: &mut SchemaContext<'_, '_>,
    path: &str,
    value: &Value,
    allowed_keys: &[&str],
) {
    let Some(table) = require_table(context, path, value, "expected table") else {
        return;
    };
    validate_allowed_keys(context, path, table, allowed_keys);

    for key in allowed_keys {
        let Some(value) = table.get(*key) else {
            continue;
        };
        if !value.is_str() {
            context.unsupported_value(
                &format!("{path}.{key}"),
                SchemaContext::value_type(value),
                "expected string",
            );
        }
    }
}
