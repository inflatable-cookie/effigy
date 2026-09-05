use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::{require_table, validate_allowed_keys};
use super::values::{
    validate_optional_boolean_field, validate_optional_enum_string_field,
    validate_optional_integer_field, validate_optional_non_empty_string_field,
    validate_optional_string_array_field,
};

pub(super) fn validate_docs_policy_section(
    context: &mut SchemaContext<'_, '_>,
    docs_policy: &Value,
) {
    let Some(docs_policy_table) = require_table(
        context,
        "docs_policy",
        docs_policy,
        "expected table with optional keys: indexes, next_actions, graph, sources",
    ) else {
        return;
    };

    validate_allowed_keys(
        context,
        "docs_policy",
        docs_policy_table,
        &[
            "indexes",
            "next_actions",
            "next-actions",
            "graph",
            "sources",
        ],
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
    if let Some(graph) = docs_policy_table.get("graph") {
        validate_docs_policy_graph(context, graph);
    }
    if let Some(sources) = docs_policy_table.get("sources") {
        validate_docs_policy_sources(context, sources);
    }
}

/// `[docs_policy.sources]` is the repository's own half of cross-repository
/// membership. It is read from these committed bytes alone, so the schema check
/// is the only place a typo in it gets caught before a portfolio silently
/// reports the repository as not shared.
fn validate_docs_policy_sources(context: &mut SchemaContext<'_, '_>, sources: &Value) {
    let Some(sources_table) = require_table(
        context,
        "docs_policy.sources",
        sources,
        "expected table with keys: share, front_doors, skill_roots",
    ) else {
        return;
    };

    validate_allowed_keys(
        context,
        "docs_policy.sources",
        sources_table,
        &[
            "share",
            "front_doors",
            "front-doors",
            "skill_roots",
            "skill-roots",
        ],
    );
    validate_optional_boolean_field(
        context,
        sources_table.get("share"),
        "docs_policy.sources.share",
    );
    validate_optional_string_array_field(
        context,
        sources_table
            .get("front_doors")
            .or_else(|| sources_table.get("front-doors")),
        "docs_policy.sources.front_doors",
        "expected array of repository-relative file paths",
    );
    validate_optional_string_array_field(
        context,
        sources_table
            .get("skill_roots")
            .or_else(|| sources_table.get("skill-roots")),
        "docs_policy.sources.skill_roots",
        "expected array of repository-relative directory paths",
    );
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

fn validate_docs_policy_graph(context: &mut SchemaContext<'_, '_>, graph: &Value) {
    let Some(graph_table) = require_table(
        context,
        "docs_policy.graph",
        graph,
        "expected table with keys: roots, fields, currentness, kinds, relations",
    ) else {
        return;
    };

    validate_allowed_keys(
        context,
        "docs_policy.graph",
        graph_table,
        &["roots", "fields", "currentness", "kinds", "relations"],
    );
    validate_optional_string_array_field(
        context,
        graph_table.get("roots"),
        "docs_policy.graph.roots",
        "expected array of strings",
    );
    if let Some(fields) = graph_table.get("fields") {
        validate_docs_policy_graph_fields(context, fields);
    }
    if let Some(currentness) = graph_table.get("currentness") {
        validate_docs_policy_graph_currentness(context, currentness);
    }
    if let Some(kinds) = graph_table.get("kinds") {
        validate_docs_policy_graph_kinds(context, kinds);
    }
    if let Some(relations) = graph_table.get("relations") {
        validate_docs_policy_graph_relations(context, relations);
    }
}

fn validate_docs_policy_graph_fields(context: &mut SchemaContext<'_, '_>, fields: &Value) {
    let Some(fields_table) = require_table(
        context,
        "docs_policy.graph.fields",
        fields,
        "expected table of named field definitions",
    ) else {
        return;
    };
    for (name, entry_value) in fields_table {
        let entry_path = format!("docs_policy.graph.fields.{name}");
        let Some(entry_table) = require_table(
            context,
            &entry_path,
            entry_value,
            "expected table with keys: labels, cardinality",
        ) else {
            continue;
        };
        validate_allowed_keys(
            context,
            &entry_path,
            entry_table,
            &["labels", "cardinality"],
        );
        validate_optional_string_array_field(
            context,
            entry_table.get("labels"),
            &format!("{entry_path}.labels"),
            "expected array of strings",
        );
        validate_optional_enum_string_field(
            context,
            entry_table.get("cardinality"),
            &format!("{entry_path}.cardinality"),
            &["one", "many"],
            "expected \"one\" or \"many\"",
        );
    }
}

fn validate_docs_policy_graph_currentness(
    context: &mut SchemaContext<'_, '_>,
    currentness: &Value,
) {
    let Some(currentness_table) = require_table(
        context,
        "docs_policy.graph.currentness",
        currentness,
        "expected table with keys: field, current, historical",
    ) else {
        return;
    };
    validate_allowed_keys(
        context,
        "docs_policy.graph.currentness",
        currentness_table,
        &["field", "current", "historical"],
    );
    validate_optional_non_empty_string_field(
        context,
        currentness_table.get("field"),
        "docs_policy.graph.currentness.field",
    );
    validate_optional_string_array_field(
        context,
        currentness_table.get("current"),
        "docs_policy.graph.currentness.current",
        "expected array of strings",
    );
    validate_optional_string_array_field(
        context,
        currentness_table.get("historical"),
        "docs_policy.graph.currentness.historical",
        "expected array of strings",
    );
}

fn validate_docs_policy_graph_kinds(context: &mut SchemaContext<'_, '_>, kinds: &Value) {
    let Some(kinds_table) = require_table(
        context,
        "docs_policy.graph.kinds",
        kinds,
        "expected table of named kind definitions",
    ) else {
        return;
    };
    for (name, entry_value) in kinds_table {
        let entry_path = format!("docs_policy.graph.kinds.{name}");
        let Some(entry_table) = require_table(
            context,
            &entry_path,
            entry_value,
            "expected table with keys: include, exclude, authority, default_currentness",
        ) else {
            continue;
        };
        validate_allowed_keys(
            context,
            &entry_path,
            entry_table,
            &[
                "include",
                "exclude",
                "authority",
                "default_currentness",
                "default-currentness",
            ],
        );
        validate_optional_string_array_field(
            context,
            entry_table.get("include"),
            &format!("{entry_path}.include"),
            "expected array of strings",
        );
        validate_optional_string_array_field(
            context,
            entry_table.get("exclude"),
            &format!("{entry_path}.exclude"),
            "expected array of strings",
        );
        validate_optional_integer_field(
            context,
            entry_table.get("authority"),
            &format!("{entry_path}.authority"),
        );
        if let Some(Value::Integer(authority)) = entry_table.get("authority") {
            if !(0..=100).contains(authority) {
                let actual = authority.to_string();
                context.unsupported_value(
                    &format!("{entry_path}.authority"),
                    &actual,
                    "expected integer from 0 through 100",
                );
            }
        }
        validate_optional_enum_string_field(
            context,
            entry_table
                .get("default_currentness")
                .or_else(|| entry_table.get("default-currentness")),
            &format!("{entry_path}.default_currentness"),
            &["current", "historical", "unknown"],
            "expected \"current\", \"historical\", or \"unknown\"",
        );
    }
}

fn validate_docs_policy_graph_relations(context: &mut SchemaContext<'_, '_>, relations: &Value) {
    let Some(relations_table) = require_table(
        context,
        "docs_policy.graph.relations",
        relations,
        "expected table of named relation definitions",
    ) else {
        return;
    };
    for (name, entry_value) in relations_table {
        let entry_path = format!("docs_policy.graph.relations.{name}");
        let Some(entry_table) = require_table(
            context,
            &entry_path,
            entry_value,
            "expected table with keys: labels, headings",
        ) else {
            continue;
        };
        validate_allowed_keys(context, &entry_path, entry_table, &["labels", "headings"]);
        validate_optional_string_array_field(
            context,
            entry_table.get("labels"),
            &format!("{entry_path}.labels"),
            "expected array of strings",
        );
        validate_optional_string_array_field(
            context,
            entry_table.get("headings"),
            &format!("{entry_path}.headings"),
            "expected array of strings",
        );
    }
}
