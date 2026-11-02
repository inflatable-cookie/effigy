use std::path::Path;

use super::parse_env_schema;
use crate::ast::{EntryAnnotations, EnvType, EnvValueExpr, TemplatePart};

fn parse(content: &str) -> crate::ast::EnvSchema {
    parse_env_schema(content, Path::new("test.env.schema")).unwrap()
}

fn parse_err(content: &str) -> String {
    parse_env_schema(content, Path::new("test.env.schema"))
        .unwrap_err()
        .to_string()
}

#[test]
fn parse_simple_literal() {
    let schema = parse("PORT=3000");
    assert_eq!(schema.entries.len(), 1);
    assert_eq!(schema.entries[0].key, "PORT");
    assert_eq!(
        schema.entries[0].value,
        Some(EnvValueExpr::Literal("3000".to_owned()))
    );
    assert_eq!(schema.entries[0].line, 1);
}

#[test]
fn parse_empty_value() {
    let schema = parse("PORT=");
    assert_eq!(schema.entries.len(), 1);
    assert_eq!(schema.entries[0].key, "PORT");
    assert!(schema.entries[0].value.is_none());
}

/// Matching single and double quotes are stripped from literal values; the
/// interior content is kept verbatim.
#[test]
fn quoted_literals_strip_matching_double_and_single_quotes() {
    for (raw, expected) in [
        ("NAME=\"hello world\"", "hello world"),
        ("NAME='hello world'", "hello world"),
    ] {
        let schema = parse(raw);
        assert_eq!(
            schema.entries[0].value,
            Some(EnvValueExpr::Literal(expected.to_owned())),
            "{raw}"
        );
    }
}

#[test]
fn parse_multiple_entries() {
    let schema = parse("PORT=3000\nHOST=localhost\nDEBUG=true");
    assert_eq!(schema.entries.len(), 3);
    assert_eq!(schema.entries[0].key, "PORT");
    assert_eq!(schema.entries[1].key, "HOST");
    assert_eq!(schema.entries[2].key, "DEBUG");
}

#[test]
fn parse_blank_lines_and_comments_ignored() {
    let content = "\n# comment\n\nPORT=3000\n\n# another comment\nHOST=localhost\n";
    let schema = parse(content);
    assert_eq!(schema.entries.len(), 2);
}

/// What a comment block must produce, compared across all `EntryAnnotations`
/// fields so stray or leaked annotations fail the case.
#[derive(Default)]
struct ExpectedAnnotations<'a> {
    env_type: Option<EnvType>,
    required: Option<bool>,
    sensitive: bool,
    description: Option<&'a str>,
    pattern: Option<&'a str>,
}

fn assert_annotations(actual: &EntryAnnotations, expected: &ExpectedAnnotations, name: &str) {
    assert_eq!(actual.env_type, expected.env_type, "{name}: env_type");
    assert_eq!(actual.required, expected.required, "{name}: required");
    assert_eq!(actual.sensitive, expected.sensitive, "{name}: sensitive");
    assert_eq!(
        actual.description.as_deref(),
        expected.description,
        "{name}: description"
    );
    assert_eq!(
        actual.pattern.as_deref(),
        expected.pattern,
        "{name}: pattern"
    );
}

fn expect_annotations(
    env_type: Option<EnvType>,
    required: Option<bool>,
) -> ExpectedAnnotations<'static> {
    ExpectedAnnotations {
        env_type,
        required,
        ..Default::default()
    }
}

/// Each annotation (and annotation combination) parses into its entry field,
/// compared across every `EntryAnnotations` field.
#[test]
fn annotations_parse_into_entry_fields() {
    let cases: [(&str, &str, ExpectedAnnotations); 11] = [
        (
            "required",
            "# @required\nPORT=3000",
            expect_annotations(None, Some(true)),
        ),
        (
            "optional",
            "# @optional\nPORT=3000",
            expect_annotations(None, Some(false)),
        ),
        (
            "sensitive",
            "# @sensitive\nSECRET=hunter2",
            ExpectedAnnotations {
                sensitive: true,
                ..Default::default()
            },
        ),
        (
            "type-port",
            "# @type=port\nPORT=3000",
            expect_annotations(Some(EnvType::Port), None),
        ),
        (
            "type-url",
            "# @type=url\nAPI_URL=https://example.com",
            expect_annotations(Some(EnvType::Url), None),
        ),
        (
            "type-enum",
            "# @type=enum(dev,staging,prod)\nENV=dev",
            expect_annotations(
                Some(EnvType::Enum(vec![
                    "dev".to_owned(),
                    "staging".to_owned(),
                    "prod".to_owned(),
                ])),
                None,
            ),
        ),
        (
            "type-string",
            "# @type=string\nNAME=hello",
            expect_annotations(
                Some(EnvType::String {
                    min_len: None,
                    max_len: None,
                }),
                None,
            ),
        ),
        (
            "string-constraints-and-pattern",
            "# @type=string @min=3 @max=8 @pattern=^[a-z]+$\nNAME=hello",
            ExpectedAnnotations {
                env_type: Some(EnvType::String {
                    min_len: Some(3),
                    max_len: Some(8),
                }),
                pattern: Some("^[a-z]+$"),
                ..Default::default()
            },
        ),
        (
            "min-before-type",
            "# @min=2 @max=5 @type=string\nNAME=hello",
            expect_annotations(
                Some(EnvType::String {
                    min_len: Some(2),
                    max_len: Some(5),
                }),
                None,
            ),
        ),
        (
            "combined",
            "# @type=port @required @sensitive\nDB_PORT=5432",
            ExpectedAnnotations {
                env_type: Some(EnvType::Port),
                required: Some(true),
                sensitive: true,
                ..Default::default()
            },
        ),
        (
            "description",
            "# The server port\n# @type=port @required\nPORT=3000",
            ExpectedAnnotations {
                description: Some("The server port"),
                env_type: Some(EnvType::Port),
                required: Some(true),
                ..Default::default()
            },
        ),
    ];

    for (name, content, expected) in cases {
        let schema = parse(content);
        assert_eq!(schema.entries.len(), 1, "{name}");
        assert_annotations(&schema.entries[0].annotations, &expected, name);
    }
}

/// Exec values accept both quote styles and keep the command verbatim.
#[test]
fn exec_values_parse_under_both_quote_styles() {
    for (raw, expected) in [
        (
            "SECRET=exec('op read \"op://vault/item\"')",
            "op read \"op://vault/item\"",
        ),
        ("SECRET=exec(\"echo hello\")", "echo hello"),
    ] {
        let schema = parse(raw);
        assert_eq!(
            schema.entries[0].value,
            Some(EnvValueExpr::Exec(expected.to_owned())),
            "{raw}"
        );
    }
}

/// Env references and templates parse to their AST, and a single-variable
/// template stays a template rather than collapsing to a literal.
#[test]
fn env_ref_and_template_values_parse_into_their_ast() {
    let schema = parse("BASE_URL=env('API_HOST')");
    assert_eq!(
        schema.entries[0].value,
        Some(EnvValueExpr::EnvRef("API_HOST".to_owned()))
    );

    let schema = parse("API_URL=https://${HOST}:${PORT}/api");
    assert_eq!(
        schema.entries[0].value,
        Some(EnvValueExpr::Template(vec![
            TemplatePart::Literal("https://".to_owned()),
            TemplatePart::VarRef("HOST".to_owned()),
            TemplatePart::Literal(":".to_owned()),
            TemplatePart::VarRef("PORT".to_owned()),
            TemplatePart::Literal("/api".to_owned()),
        ]))
    );

    let schema = parse("ALIAS=${OTHER_VAR}");
    assert_eq!(
        schema.entries[0].value,
        Some(EnvValueExpr::Template(vec![TemplatePart::VarRef(
            "OTHER_VAR".to_owned()
        )]))
    );
}

/// A schema source the parser must refuse, and the phrase its error must name.
struct ParseErrorCase {
    name: &'static str,
    content: &'static str,
    expected: &'static str,
}

/// Every malformed key, value expression and annotation is refused with the
/// remedy in the message rather than dropped.
#[test]
fn malformed_schema_sources_are_refused_with_their_reason() {
    let cases = [
        ParseErrorCase {
            name: "invalid-key",
            content: "123BAD=value",
            expected: "invalid variable name",
        },
        ParseErrorCase {
            name: "empty-key",
            content: "=value",
            expected: "empty variable name",
        },
        ParseErrorCase {
            name: "no-equals",
            content: "NOEQUALS",
            expected: "expected KEY=VALUE",
        },
        ParseErrorCase {
            name: "unclosed-template",
            content: "URL=https://${HOST/path",
            expected: "unclosed",
        },
        ParseErrorCase {
            name: "empty-template-variable",
            content: "URL=prefix${}suffix",
            expected: "empty variable reference",
        },
        ParseErrorCase {
            name: "unknown-annotation",
            content: "# @bogus\nPORT=3000",
            expected: "unknown annotation",
        },
        ParseErrorCase {
            name: "unknown-type",
            content: "# @type=integer\nPORT=3000",
            expected: "unknown type",
        },
        ParseErrorCase {
            name: "min-requires-an-integer",
            content: "# @type=string @min=abc\nNAME=hello",
            expected: "@min requires a non-negative integer",
        },
        ParseErrorCase {
            name: "bounds-require-string-type",
            content: "# @type=port @min=2\nPORT=3000",
            expected: "@min`/`@max` require `@type=string`",
        },
        ParseErrorCase {
            name: "min-exceeds-max",
            content: "# @type=string @min=8 @max=3\nNAME=hello",
            expected: "cannot exceed",
        },
        ParseErrorCase {
            name: "invalid-pattern-regex",
            content: "# @pattern=[a-\nNAME=hello",
            expected: "invalid `@pattern` regex",
        },
    ];

    for case in cases {
        let error = parse_err(case.content);
        assert!(
            error.contains(case.expected),
            "{}: expected `{}` in `{error}`",
            case.name,
            case.expected
        );
    }
}

/// Empty and comment-only sources both parse to an empty schema.
#[test]
fn empty_and_comment_only_schemas_have_no_entries() {
    assert!(parse("").entries.is_empty());
    assert!(parse("# just a comment\n# nothing else").entries.is_empty());
}

#[test]
fn parse_blank_line_resets_annotations() {
    let content = "# @sensitive\n\nPORT=3000";
    let schema = parse(content);
    assert!(!schema.entries[0].annotations.sensitive);
}

#[test]
fn parse_whitespace_around_key_and_value() {
    let schema = parse("  PORT  =  3000  ");
    assert_eq!(schema.entries[0].key, "PORT");
    assert_eq!(
        schema.entries[0].value,
        Some(EnvValueExpr::Literal("3000".to_owned()))
    );
}

#[test]
fn parse_underscore_key() {
    let schema = parse("_INTERNAL_VAR=value");
    assert_eq!(schema.entries[0].key, "_INTERNAL_VAR");
}

#[test]
fn parse_line_numbers_correct() {
    let content = "# comment\n\nFIRST=1\n\nSECOND=2";
    let schema = parse(content);
    assert_eq!(schema.entries[0].line, 3);
    assert_eq!(schema.entries[1].line, 5);
}
