use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::require_table;

/// `[sources.<name>]` — one pinned extra source (contract 020).
pub(super) fn validate_sources_section(context: &mut SchemaContext<'_, '_>, value: &Value) {
    let Some(sources) = value.as_table() else {
        context.unsupported_value("sources", "wrong value type", "table of pinned sources");
        return;
    };
    for (name, entry) in sources {
        let path = format!("sources.{name}");
        if name.trim().is_empty() || name.contains('/') || name.contains(':') {
            context.unsupported_value(&path, "a handle with a '/' or ':'", "a plain source name");
            continue;
        }
        let Some(table) = require_table(context, &path, entry, "table") else {
            continue;
        };
        for key in table.keys() {
            if !matches!(
                key.as_str(),
                "git" | "rev" | "rev_file" | "rev-file" | "path"
            ) {
                context.unsupported_nested_key(&path, key);
            }
        }
        for required in ["git", "path"] {
            match table.get(required) {
                Some(Value::String(value)) if !value.trim().is_empty() => {}
                Some(_) => context.unsupported_value(
                    &format!("{path}.{required}"),
                    "empty or non-string value",
                    "a non-empty string",
                ),
                None => context.unsupported_value(
                    &format!("{path}.{required}"),
                    "missing value",
                    "a non-empty string",
                ),
            }
        }
        let inline = table
            .get("rev")
            .and_then(Value::as_str)
            .is_some_and(|rev| !rev.trim().is_empty());
        let from_file = ["rev_file", "rev-file"].iter().any(|key| {
            table
                .get(*key)
                .and_then(Value::as_str)
                .is_some_and(|file| !file.trim().is_empty())
        });
        if inline && from_file {
            context.unsupported_value(
                &path,
                "both `rev` and `rev_file`",
                "exactly one of `rev` or `rev_file`",
            );
        } else if !inline && !from_file {
            context.unsupported_value(
                &path,
                "no pin",
                "exactly one of `rev` or `rev_file`; a source is never advanced silently",
            );
        }
    }
}
