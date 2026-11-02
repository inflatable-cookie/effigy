use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use super::{resolve_schema, ResolutionContext, ResolvedSource};
use crate::ast::{EntryAnnotations, EnvSchema, EnvSchemaEntry, EnvValueExpr, TemplatePart};

fn make_context() -> ResolutionContext<'static> {
    ResolutionContext {
        process_env: BTreeMap::new(),
        dotenv_overrides: BTreeMap::new(),
        exec_timeout: Duration::from_secs(5),
        project_root: PathBuf::from("/tmp"),
        exec: None,
    }
}

fn make_context_with_timeout(timeout: Duration) -> ResolutionContext<'static> {
    ResolutionContext {
        exec_timeout: timeout,
        ..make_context()
    }
}

fn make_context_with_sources(
    process_env: &[(&str, &str)],
    dotenv: &[(&str, &str)],
) -> ResolutionContext<'static> {
    let mut ctx = make_context();
    for (key, value) in process_env {
        ctx.process_env
            .insert((*key).to_owned(), (*value).to_owned());
    }
    for (key, value) in dotenv {
        ctx.dotenv_overrides
            .insert((*key).to_owned(), (*value).to_owned());
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

fn port_schema() -> EnvSchema {
    schema(vec![entry(
        "PORT",
        Some(EnvValueExpr::Literal("3000".to_owned())),
    )])
}

/// Resolution reads the process env first, then dotenv overrides, then the
/// schema value — each row names the sources it pits against each other.
#[test]
fn resolution_priority_runs_process_env_then_dotenv_then_schema() {
    struct PriorityRow {
        name: &'static str,
        process_env: &'static [(&'static str, &'static str)],
        dotenv: &'static [(&'static str, &'static str)],
        value: &'static str,
        source: ResolvedSource,
    }

    let rows = [
        PriorityRow {
            name: "schema-default",
            process_env: &[],
            dotenv: &[],
            value: "3000",
            source: ResolvedSource::SchemaDefault,
        },
        PriorityRow {
            name: "process-env-over-schema",
            process_env: &[("PORT", "8080")],
            dotenv: &[],
            value: "8080",
            source: ResolvedSource::ProcessEnv,
        },
        PriorityRow {
            name: "dotenv-over-schema",
            process_env: &[],
            dotenv: &[("PORT", "9090")],
            value: "9090",
            source: ResolvedSource::DotenvOverride,
        },
        PriorityRow {
            name: "process-env-over-dotenv",
            process_env: &[("PORT", "8080")],
            dotenv: &[("PORT", "9090")],
            value: "8080",
            source: ResolvedSource::ProcessEnv,
        },
    ];

    for row in &rows {
        let ctx = make_context_with_sources(row.process_env, row.dotenv);
        let resolved = resolve_schema(&port_schema(), &ctx).unwrap();
        assert_eq!(resolved.get_value("PORT"), Some(row.value), "{}", row.name);
        assert_eq!(
            resolved.get("PORT").unwrap().source,
            row.source,
            "{}",
            row.name
        );
    }
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

/// Template variables interpolate schema entries and fall back to the process
/// environment for names the schema does not declare.
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

    let s = schema(vec![entry(
        "URL",
        Some(EnvValueExpr::Template(vec![
            TemplatePart::Literal("https://".to_owned()),
            TemplatePart::VarRef("EXT_HOST".to_owned()),
        ])),
    )]);
    let ctx = make_context_with_sources(&[("EXT_HOST", "example.com")], &[]);
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

/// An entry with no value is skipped unless it is `@required`, which is a
/// resolution error naming the key.
#[test]
fn missing_entries_follow_the_required_annotation() {
    let s = schema(vec![entry("MAYBE", None)]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();
    assert!(!resolved.contains_key("MAYBE"));

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
