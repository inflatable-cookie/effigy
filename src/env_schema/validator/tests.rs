use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use super::validate_resolved_env;
use crate::env_schema::ast::{EntryAnnotations, EnvSchema, EnvSchemaEntry, EnvType, EnvValueExpr};
use crate::env_schema::resolver::{resolve_schema, ResolutionContext};

fn make_context() -> ResolutionContext {
    ResolutionContext {
        process_env: BTreeMap::new(),
        dotenv_overrides: BTreeMap::new(),
        exec_timeout: Duration::from_secs(5),
        project_root: PathBuf::from("/tmp"),
    }
}

fn typed_entry(key: &str, value: &str, env_type: EnvType) -> EnvSchemaEntry {
    EnvSchemaEntry {
        key: key.to_owned(),
        value: Some(EnvValueExpr::Literal(value.to_owned())),
        annotations: EntryAnnotations {
            env_type: Some(env_type),
            ..EntryAnnotations::default()
        },
        line: 1,
    }
}

fn schema(entries: Vec<EnvSchemaEntry>) -> EnvSchema {
    EnvSchema {
        entries,
        source_path: PathBuf::from("test.env.schema"),
    }
}

#[test]
fn valid_port() {
    let s = schema(vec![typed_entry("PORT", "3000", EnvType::Port)]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();
    let errors = validate_resolved_env(&s, &resolved);
    assert!(errors.is_empty());
}

#[test]
fn valid_port_edge_1() {
    let s = schema(vec![typed_entry("PORT", "1", EnvType::Port)]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();
    let errors = validate_resolved_env(&s, &resolved);
    assert!(errors.is_empty());
}

#[test]
fn valid_port_edge_65535() {
    let s = schema(vec![typed_entry("PORT", "65535", EnvType::Port)]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();
    let errors = validate_resolved_env(&s, &resolved);
    assert!(errors.is_empty());
}

#[test]
fn invalid_port_zero() {
    let s = schema(vec![typed_entry("PORT", "0", EnvType::Port)]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();
    let errors = validate_resolved_env(&s, &resolved);
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0].expected.contains("port"),
        "{}",
        errors[0].expected
    );
}

#[test]
fn invalid_port_too_high() {
    let s = schema(vec![typed_entry("PORT", "70000", EnvType::Port)]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();
    let errors = validate_resolved_env(&s, &resolved);
    assert_eq!(errors.len(), 1);
}

#[test]
fn invalid_port_not_number() {
    let s = schema(vec![typed_entry("PORT", "abc", EnvType::Port)]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();
    let errors = validate_resolved_env(&s, &resolved);
    assert_eq!(errors.len(), 1);
}

#[test]
fn valid_url() {
    let s = schema(vec![typed_entry(
        "API",
        "https://example.com",
        EnvType::Url,
    )]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();
    let errors = validate_resolved_env(&s, &resolved);
    assert!(errors.is_empty());
}

#[test]
fn valid_url_with_path() {
    let s = schema(vec![typed_entry(
        "API",
        "http://localhost:3000/api/v1",
        EnvType::Url,
    )]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();
    let errors = validate_resolved_env(&s, &resolved);
    assert!(errors.is_empty());
}

#[test]
fn invalid_url_no_scheme() {
    let s = schema(vec![typed_entry("API", "example.com", EnvType::Url)]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();
    let errors = validate_resolved_env(&s, &resolved);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].expected.contains("URL"), "{}", errors[0].expected);
}

#[test]
fn invalid_url_empty_host() {
    let s = schema(vec![typed_entry("API", "https://", EnvType::Url)]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();
    let errors = validate_resolved_env(&s, &resolved);
    assert_eq!(errors.len(), 1);
}

#[test]
fn valid_enum() {
    let s = schema(vec![typed_entry(
        "ENV",
        "prod",
        EnvType::Enum(vec![
            "dev".to_owned(),
            "staging".to_owned(),
            "prod".to_owned(),
        ]),
    )]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();
    let errors = validate_resolved_env(&s, &resolved);
    assert!(errors.is_empty());
}

#[test]
fn invalid_enum() {
    let s = schema(vec![typed_entry(
        "ENV",
        "test",
        EnvType::Enum(vec![
            "dev".to_owned(),
            "staging".to_owned(),
            "prod".to_owned(),
        ]),
    )]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();
    let errors = validate_resolved_env(&s, &resolved);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].expected.contains("dev"), "{}", errors[0].expected);
}

#[test]
fn valid_string_no_constraints() {
    let s = schema(vec![typed_entry(
        "NAME",
        "hello",
        EnvType::String {
            min_len: None,
            max_len: None,
        },
    )]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();
    let errors = validate_resolved_env(&s, &resolved);
    assert!(errors.is_empty());
}

#[test]
fn no_validation_without_type() {
    let s = schema(vec![EnvSchemaEntry {
        key: "UNTYPED".to_owned(),
        value: Some(EnvValueExpr::Literal("anything".to_owned())),
        annotations: EntryAnnotations::default(),
        line: 1,
    }]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();
    let errors = validate_resolved_env(&s, &resolved);
    assert!(errors.is_empty());
}

#[test]
fn unresolved_entry_skipped_in_validation() {
    let s = schema(vec![EnvSchemaEntry {
        key: "MISSING".to_owned(),
        value: None,
        annotations: EntryAnnotations {
            env_type: Some(EnvType::Port),
            ..EntryAnnotations::default()
        },
        line: 1,
    }]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();
    let errors = validate_resolved_env(&s, &resolved);
    assert!(errors.is_empty());
}
