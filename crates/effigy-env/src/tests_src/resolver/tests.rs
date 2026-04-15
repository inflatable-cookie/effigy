use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use super::{resolve_schema, ResolutionContext, ResolvedSource};
use crate::ast::{EntryAnnotations, EnvSchema, EnvSchemaEntry, EnvValueExpr, TemplatePart};

fn make_context() -> ResolutionContext {
    ResolutionContext {
        process_env: BTreeMap::new(),
        dotenv_overrides: BTreeMap::new(),
        exec_timeout: Duration::from_secs(5),
        project_root: PathBuf::from("/tmp"),
    }
}

fn make_context_with_timeout(timeout: Duration) -> ResolutionContext {
    ResolutionContext {
        exec_timeout: timeout,
        ..make_context()
    }
}

fn make_context_with_env(vars: &[(&str, &str)]) -> ResolutionContext {
    let mut ctx = make_context();
    for (k, v) in vars {
        ctx.process_env.insert((*k).to_owned(), (*v).to_owned());
    }
    ctx
}

fn make_context_with_dotenv(vars: &[(&str, &str)]) -> ResolutionContext {
    let mut ctx = make_context();
    for (k, v) in vars {
        ctx.dotenv_overrides
            .insert((*k).to_owned(), (*v).to_owned());
    }
    ctx
}

fn entry(key: &str, value: Option<EnvValueExpr>) -> EnvSchemaEntry {
    EnvSchemaEntry {
        key: key.to_owned(),
        value,
        annotations: EntryAnnotations::default(),
        line: 1,
    }
}

fn entry_sensitive(key: &str, value: Option<EnvValueExpr>) -> EnvSchemaEntry {
    EnvSchemaEntry {
        key: key.to_owned(),
        value,
        annotations: EntryAnnotations {
            sensitive: true,
            ..EntryAnnotations::default()
        },
        line: 1,
    }
}

fn entry_required(key: &str, value: Option<EnvValueExpr>) -> EnvSchemaEntry {
    EnvSchemaEntry {
        key: key.to_owned(),
        value,
        annotations: EntryAnnotations {
            required: Some(true),
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
fn resolve_literal_default() {
    let s = schema(vec![entry(
        "PORT",
        Some(EnvValueExpr::Literal("3000".to_owned())),
    )]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();
    assert_eq!(resolved.get_value("PORT"), Some("3000"));
    assert_eq!(
        resolved.get("PORT").unwrap().source,
        ResolvedSource::SchemaDefault
    );
}

#[test]
fn resolve_process_env_takes_priority() {
    let s = schema(vec![entry(
        "PORT",
        Some(EnvValueExpr::Literal("3000".to_owned())),
    )]);
    let ctx = make_context_with_env(&[("PORT", "8080")]);
    let resolved = resolve_schema(&s, &ctx).unwrap();
    assert_eq!(resolved.get_value("PORT"), Some("8080"));
    assert_eq!(
        resolved.get("PORT").unwrap().source,
        ResolvedSource::ProcessEnv
    );
}

#[test]
fn resolve_dotenv_overrides_schema_default() {
    let s = schema(vec![entry(
        "PORT",
        Some(EnvValueExpr::Literal("3000".to_owned())),
    )]);
    let ctx = make_context_with_dotenv(&[("PORT", "9090")]);
    let resolved = resolve_schema(&s, &ctx).unwrap();
    assert_eq!(resolved.get_value("PORT"), Some("9090"));
    assert_eq!(
        resolved.get("PORT").unwrap().source,
        ResolvedSource::DotenvOverride
    );
}

#[test]
fn resolve_process_env_over_dotenv() {
    let s = schema(vec![entry(
        "PORT",
        Some(EnvValueExpr::Literal("3000".to_owned())),
    )]);
    let mut ctx = make_context_with_env(&[("PORT", "8080")]);
    ctx.dotenv_overrides
        .insert("PORT".to_owned(), "9090".to_owned());
    let resolved = resolve_schema(&s, &ctx).unwrap();
    assert_eq!(resolved.get_value("PORT"), Some("8080"));
}

#[test]
fn resolve_env_ref() {
    let s = schema(vec![
        entry("HOST", Some(EnvValueExpr::Literal("localhost".to_owned()))),
        entry("ALIAS", Some(EnvValueExpr::EnvRef("HOST".to_owned()))),
    ]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();
    assert_eq!(resolved.get_value("ALIAS"), Some("localhost"));
    assert_eq!(
        resolved.get("ALIAS").unwrap().source,
        ResolvedSource::EnvReference
    );
}

#[test]
fn resolve_template() {
    let s = schema(vec![
        entry("HOST", Some(EnvValueExpr::Literal("localhost".to_owned()))),
        entry("PORT", Some(EnvValueExpr::Literal("3000".to_owned()))),
        entry(
            "URL",
            Some(EnvValueExpr::Template(vec![
                TemplatePart::Literal("http://".to_owned()),
                TemplatePart::VarRef("HOST".to_owned()),
                TemplatePart::Literal(":".to_owned()),
                TemplatePart::VarRef("PORT".to_owned()),
            ])),
        ),
    ]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();
    assert_eq!(resolved.get_value("URL"), Some("http://localhost:3000"));
    assert_eq!(
        resolved.get("URL").unwrap().source,
        ResolvedSource::TemplateInterpolation
    );
}

#[test]
fn resolve_template_with_process_env() {
    let s = schema(vec![entry(
        "URL",
        Some(EnvValueExpr::Template(vec![
            TemplatePart::Literal("https://".to_owned()),
            TemplatePart::VarRef("EXT_HOST".to_owned()),
        ])),
    )]);
    let ctx = make_context_with_env(&[("EXT_HOST", "example.com")]);
    let resolved = resolve_schema(&s, &ctx).unwrap();
    assert_eq!(resolved.get_value("URL"), Some("https://example.com"));
}

#[test]
fn resolve_exec_echo() {
    let s = schema(vec![entry(
        "ECHO_VAL",
        Some(EnvValueExpr::Exec("echo hello".to_owned())),
    )]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();
    assert_eq!(resolved.get_value("ECHO_VAL"), Some("hello"));
    assert_eq!(
        resolved.get("ECHO_VAL").unwrap().source,
        ResolvedSource::ExecCommand
    );
}

#[test]
fn resolve_exec_failure() {
    let s = schema(vec![entry(
        "FAIL",
        Some(EnvValueExpr::Exec("exit 1".to_owned())),
    )]);
    let err = resolve_schema(&s, &make_context()).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("exit 1"), "got: {msg}");
}

#[test]
fn resolve_exec_timeout() {
    let s = schema(vec![entry(
        "SLOW",
        Some(EnvValueExpr::Exec("sleep 1".to_owned())),
    )]);
    let err = resolve_schema(&s, &make_context_with_timeout(Duration::from_millis(50)))
        .expect_err("exec should time out");
    let msg = err.to_string();
    assert!(msg.contains("timed out"), "got: {msg}");
    assert!(msg.contains("sleep 1"), "got: {msg}");
}

#[test]
fn resolve_circular_dependency_detected() {
    let s = schema(vec![
        entry("A", Some(EnvValueExpr::EnvRef("B".to_owned()))),
        entry("B", Some(EnvValueExpr::EnvRef("A".to_owned()))),
    ]);
    let err = resolve_schema(&s, &make_context()).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("circular dependency"), "got: {msg}");
}

#[test]
fn resolve_missing_optional_is_ok() {
    let s = schema(vec![entry("MAYBE", None)]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();
    assert!(!resolved.contains_key("MAYBE"));
}

#[test]
fn resolve_missing_required_is_error() {
    let s = schema(vec![entry_required("MUST_HAVE", None)]);
    let err = resolve_schema(&s, &make_context()).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("required"), "got: {msg}");
    assert!(msg.contains("MUST_HAVE"), "got: {msg}");
}

#[test]
fn resolve_sensitive_produces_secret() {
    let s = schema(vec![entry_sensitive(
        "TOKEN",
        Some(EnvValueExpr::Literal("abc123".to_owned())),
    )]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();
    let entry = resolved.get("TOKEN").unwrap();
    assert!(entry.value.is_secret());
    assert_eq!(entry.value.as_str(), "abc123");
    assert_eq!(format!("{}", entry.value), "[REDACTED]");
}

#[test]
fn resolve_plain_and_secret_separation() {
    let s = schema(vec![
        entry("HOST", Some(EnvValueExpr::Literal("localhost".to_owned()))),
        entry_sensitive("SECRET", Some(EnvValueExpr::Literal("hidden".to_owned()))),
    ]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();
    let plain = resolved.plain_env();
    let secrets = resolved.secret_env();

    assert_eq!(plain.len(), 1);
    assert_eq!(plain.get("HOST"), Some(&"localhost".to_owned()));

    assert_eq!(secrets.len(), 1);
    assert_eq!(secrets[0].0, "SECRET");
    assert_eq!(secrets[0].1.expose(), "hidden");
}

#[test]
fn resolve_debug_output_redacts_secret_values() {
    let s = schema(vec![
        entry("HOST", Some(EnvValueExpr::Literal("localhost".to_owned()))),
        entry_sensitive(
            "SECRET",
            Some(EnvValueExpr::Literal("hidden-token".to_owned())),
        ),
    ]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();

    let debug = format!("{resolved:?}");
    assert!(debug.contains("SecretString(****)"), "got: {debug}");
    assert!(!debug.contains("hidden-token"), "secret leaked: {debug}");
}

#[test]
fn resolve_dependency_order_respected() {
    // C depends on B, B depends on A — should resolve in order A, B, C
    let s = schema(vec![
        entry(
            "C",
            Some(EnvValueExpr::Template(vec![
                TemplatePart::VarRef("B".to_owned()),
                TemplatePart::Literal("/c".to_owned()),
            ])),
        ),
        entry("A", Some(EnvValueExpr::Literal("base".to_owned()))),
        entry(
            "B",
            Some(EnvValueExpr::Template(vec![
                TemplatePart::VarRef("A".to_owned()),
                TemplatePart::Literal("/b".to_owned()),
            ])),
        ),
    ]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();
    assert_eq!(resolved.get_value("A"), Some("base"));
    assert_eq!(resolved.get_value("B"), Some("base/b"));
    assert_eq!(resolved.get_value("C"), Some("base/b/c"));
}
