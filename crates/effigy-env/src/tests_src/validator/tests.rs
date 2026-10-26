use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use super::validate_resolved_env;
use crate::ast::{EntryAnnotations, EnvSchema, EnvSchemaEntry, EnvType, EnvValueExpr};
use crate::resolver::{resolve_schema, ResolutionContext};

fn make_context() -> ResolutionContext<'static> {
    ResolutionContext {
        process_env: BTreeMap::new(),
        dotenv_overrides: BTreeMap::new(),
        exec_timeout: Duration::from_secs(5),
        project_root: PathBuf::from("/tmp"),
        exec: None,
    }
}

fn schema(entries: Vec<EnvSchemaEntry>) -> EnvSchema {
    EnvSchema {
        entries,
        source_path: PathBuf::from("test.env.schema"),
    }
}

/// What a typed entry must produce from the validator.
enum Expectation {
    Valid,
    Invalid {
        errors: usize,
        expected: Option<&'static str>,
    },
}

/// One typed or patterned entry and its validation verdict.
struct ValidationCase {
    name: &'static str,
    entry: EnvSchemaEntry,
    expectation: Expectation,
}

fn literal(key: &str, value: &str, annotations: EntryAnnotations) -> EnvSchemaEntry {
    EnvSchemaEntry {
        key: key.to_owned(),
        value: Some(EnvValueExpr::Literal(value.to_owned())),
        annotations,
        line: 1,
    }
}

fn typed(key: &str, value: &str, env_type: EnvType) -> EnvSchemaEntry {
    literal(
        key,
        value,
        EntryAnnotations {
            env_type: Some(env_type),
            ..EntryAnnotations::default()
        },
    )
}

fn patterned(key: &str, value: &str, pattern: &str) -> EnvSchemaEntry {
    literal(
        key,
        value,
        EntryAnnotations {
            pattern: Some(pattern.to_owned()),
            ..EntryAnnotations::default()
        },
    )
}

/// Each type validates its own extremes and refusals, and a pattern runs with or
/// without a declared type.
#[test]
fn typed_and_patterned_entries_validate_by_type() {
    let cases = vec![
        ValidationCase {
            name: "port",
            entry: typed("PORT", "3000", EnvType::Port),
            expectation: Expectation::Valid,
        },
        ValidationCase {
            name: "port-minimum",
            entry: typed("PORT", "1", EnvType::Port),
            expectation: Expectation::Valid,
        },
        ValidationCase {
            name: "port-maximum",
            entry: typed("PORT", "65535", EnvType::Port),
            expectation: Expectation::Valid,
        },
        ValidationCase {
            name: "port-zero",
            entry: typed("PORT", "0", EnvType::Port),
            expectation: Expectation::Invalid {
                errors: 1,
                expected: Some("port"),
            },
        },
        ValidationCase {
            name: "port-too-high",
            entry: typed("PORT", "70000", EnvType::Port),
            expectation: Expectation::Invalid {
                errors: 1,
                expected: None,
            },
        },
        ValidationCase {
            name: "port-not-a-number",
            entry: typed("PORT", "abc", EnvType::Port),
            expectation: Expectation::Invalid {
                errors: 1,
                expected: None,
            },
        },
        ValidationCase {
            name: "url",
            entry: typed("API", "https://example.com", EnvType::Url),
            expectation: Expectation::Valid,
        },
        ValidationCase {
            name: "url-with-path",
            entry: typed("API", "http://localhost:3000/api/v1", EnvType::Url),
            expectation: Expectation::Valid,
        },
        ValidationCase {
            name: "url-without-scheme",
            entry: typed("API", "example.com", EnvType::Url),
            expectation: Expectation::Invalid {
                errors: 1,
                expected: Some("URL"),
            },
        },
        ValidationCase {
            name: "url-without-host",
            entry: typed("API", "https://", EnvType::Url),
            expectation: Expectation::Invalid {
                errors: 1,
                expected: None,
            },
        },
        ValidationCase {
            name: "enum-member",
            entry: typed(
                "ENV",
                "prod",
                EnvType::Enum(vec![
                    "dev".to_owned(),
                    "staging".to_owned(),
                    "prod".to_owned(),
                ]),
            ),
            expectation: Expectation::Valid,
        },
        ValidationCase {
            name: "enum-non-member",
            entry: typed(
                "ENV",
                "test",
                EnvType::Enum(vec![
                    "dev".to_owned(),
                    "staging".to_owned(),
                    "prod".to_owned(),
                ]),
            ),
            expectation: Expectation::Invalid {
                errors: 1,
                expected: Some("dev"),
            },
        },
        ValidationCase {
            name: "string-unconstrained",
            entry: typed(
                "NAME",
                "hello",
                EnvType::String {
                    min_len: None,
                    max_len: None,
                },
            ),
            expectation: Expectation::Valid,
        },
        ValidationCase {
            name: "string-within-bounds",
            entry: typed(
                "NAME",
                "hello",
                EnvType::String {
                    min_len: Some(3),
                    max_len: Some(8),
                },
            ),
            expectation: Expectation::Valid,
        },
        ValidationCase {
            name: "string-too-short",
            entry: typed(
                "NAME",
                "hi",
                EnvType::String {
                    min_len: Some(3),
                    max_len: None,
                },
            ),
            expectation: Expectation::Invalid {
                errors: 1,
                expected: Some("min length 3"),
            },
        },
        ValidationCase {
            name: "string-too-long",
            entry: typed(
                "NAME",
                "toolong",
                EnvType::String {
                    min_len: None,
                    max_len: Some(4),
                },
            ),
            expectation: Expectation::Invalid {
                errors: 1,
                expected: Some("max length 4"),
            },
        },
        ValidationCase {
            name: "pattern-matches",
            entry: patterned(
                "API_URL",
                "https://service.example.test",
                "^https://[a-z]+\\.example\\.test$",
            ),
            expectation: Expectation::Valid,
        },
        ValidationCase {
            name: "pattern-misses",
            entry: patterned(
                "API_URL",
                "http://service.example.test",
                "^https://[a-z]+\\.example\\.test$",
            ),
            expectation: Expectation::Invalid {
                errors: 1,
                expected: Some("pattern /"),
            },
        },
        ValidationCase {
            name: "type-and-pattern-both-report",
            entry: literal(
                "API_URL",
                "nope",
                EntryAnnotations {
                    env_type: Some(EnvType::String {
                        min_len: Some(8),
                        max_len: None,
                    }),
                    pattern: Some("^https://".to_owned()),
                    ..EntryAnnotations::default()
                },
            ),
            expectation: Expectation::Invalid {
                errors: 2,
                expected: None,
            },
        },
        ValidationCase {
            name: "no-type-no-validation",
            entry: literal("UNTYPED", "anything", EntryAnnotations::default()),
            expectation: Expectation::Valid,
        },
        ValidationCase {
            name: "pattern-without-type",
            entry: patterned("HOST", "localhost", "^api\\."),
            expectation: Expectation::Invalid {
                errors: 1,
                expected: None,
            },
        },
    ];

    for case in cases {
        let s = schema(vec![case.entry]);
        let resolved = resolve_schema(&s, &make_context()).expect("resolve entry");
        let errors = validate_resolved_env(&s, &resolved);
        match case.expectation {
            Expectation::Valid => {
                assert!(errors.is_empty(), "{}: {errors:?}", case.name);
            }
            Expectation::Invalid {
                errors: count,
                expected,
            } => {
                assert_eq!(errors.len(), count, "{}: {errors:?}", case.name);
                if let Some(fragment) = expected {
                    assert!(
                        errors[0].expected.contains(fragment),
                        "{}: expected `{fragment}` in `{}`",
                        case.name,
                        errors[0].expected
                    );
                }
            }
        }
    }
}

#[test]
fn sensitive_validation_error_redacts_actual_value() {
    let s = schema(vec![EnvSchemaEntry {
        key: "API_TOKEN".to_owned(),
        value: Some(EnvValueExpr::Literal("super-secret-token".to_owned())),
        annotations: EntryAnnotations {
            sensitive: true,
            pattern: Some("^tok_[a-z0-9]+$".to_owned()),
            ..EntryAnnotations::default()
        },
        line: 12,
    }]);
    let resolved = resolve_schema(&s, &make_context()).unwrap();
    let errors = validate_resolved_env(&s, &resolved);

    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].actual, "[REDACTED]");
    let rendered = errors[0].to_string();
    assert!(rendered.contains("[REDACTED]"), "got: {rendered}");
    assert!(
        !rendered.contains("super-secret-token"),
        "sensitive value leaked: {rendered}"
    );
    let debug = format!("{:?}", errors[0]);
    assert!(
        !debug.contains("super-secret-token"),
        "sensitive value leaked in debug: {debug}"
    );
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
