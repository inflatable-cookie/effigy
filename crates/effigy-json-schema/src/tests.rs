use serde_json::json;

use super::{validate, CompiledSchema};

#[test]
fn valid_instance_reports_no_errors() {
    let schema = json!({
        "type": "object",
        "required": ["count"],
        "properties": {
            "count": { "type": "number" },
            "mode": { "enum": ["fast", "full"] },
        },
        "additionalProperties": false,
    });
    let instance = json!({ "count": 2, "mode": "full" });

    assert!(validate(&schema, &instance)
        .expect("valid schema")
        .is_empty());
}

#[test]
fn missing_required_key_reports_the_document_root() {
    let schema = json!({ "type": "object", "required": ["count", "mode"] });
    let errors = validate(&schema, &json!({})).expect("valid schema");

    assert_eq!(errors.len(), 2, "unexpected errors: {errors:?}");
    assert!(errors.iter().all(|error| error.starts_with("<root>: ")));
    assert!(errors.iter().any(|error| error.contains("count")));
    assert!(errors.iter().any(|error| error.contains("mode")));
}

#[test]
fn wrong_nested_type_reports_the_json_pointer() {
    let schema = json!({
        "type": "object",
        "properties": {
            "selected": { "type": "array", "items": { "type": "string" } }
        }
    });
    let errors = validate(&schema, &json!({ "selected": ["a", 7] })).expect("valid schema");

    assert_eq!(errors.len(), 1, "unexpected errors: {errors:?}");
    assert!(
        errors[0].starts_with("/selected/1: "),
        "unexpected: {errors:?}"
    );
}

#[test]
fn format_is_annotation_only() {
    let schema = json!({ "type": "string", "format": "uri" });

    assert!(validate(&schema, &json!("not a uri"))
        .expect("valid schema")
        .is_empty());
}

#[test]
fn in_document_refs_resolve() {
    let schema = json!({
        "$defs": { "entry": { "type": "string", "minLength": 1 } },
        "type": "object",
        "properties": { "name": { "$ref": "#/$defs/entry" } }
    });

    assert!(validate(&schema, &json!({ "name": "ok" }))
        .expect("valid schema")
        .is_empty());
    let errors = validate(&schema, &json!({ "name": "" })).expect("valid schema");
    assert_eq!(errors.len(), 1, "unexpected errors: {errors:?}");
    assert!(errors[0].starts_with("/name: "), "unexpected: {errors:?}");
}

#[test]
fn external_refs_are_rejected_instead_of_fetched() {
    let schema = json!({ "$ref": "https://example.test/other.json" });
    let error = CompiledSchema::compile(&schema).expect_err("remote ref must not resolve");

    assert!(!error.message.is_empty());
}

#[test]
fn invalid_schemas_are_errors_not_passes() {
    let schema = json!({ "type": "object", "required": "count" });

    assert!(CompiledSchema::compile(&schema).is_err());
}

#[test]
fn errors_are_sorted_and_deduplicated() {
    let schema = json!({
        "type": "object",
        "properties": {
            "b": { "type": "string" },
            "a": { "type": "string" }
        }
    });
    let errors = validate(&schema, &json!({ "a": 1, "b": 2 })).expect("valid schema");

    let mut sorted = errors.clone();
    sorted.sort();
    assert_eq!(errors, sorted);
    assert_eq!(errors.len(), 2, "unexpected errors: {errors:?}");
}
