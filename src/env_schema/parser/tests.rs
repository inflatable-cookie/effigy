use std::path::Path;

use super::parse_env_schema;
use crate::env_schema::ast::{EnvType, EnvValueExpr, TemplatePart};

fn parse(content: &str) -> crate::env_schema::ast::EnvSchema {
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

#[test]
fn parse_quoted_value() {
    let schema = parse("NAME=\"hello world\"");
    assert_eq!(
        schema.entries[0].value,
        Some(EnvValueExpr::Literal("hello world".to_owned()))
    );
}

#[test]
fn parse_single_quoted_value() {
    let schema = parse("NAME='hello world'");
    assert_eq!(
        schema.entries[0].value,
        Some(EnvValueExpr::Literal("hello world".to_owned()))
    );
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

#[test]
fn parse_annotation_required() {
    let content = "# @required\nPORT=3000";
    let schema = parse(content);
    assert_eq!(schema.entries[0].annotations.required, Some(true));
}

#[test]
fn parse_annotation_optional() {
    let content = "# @optional\nPORT=3000";
    let schema = parse(content);
    assert_eq!(schema.entries[0].annotations.required, Some(false));
}

#[test]
fn parse_annotation_sensitive() {
    let content = "# @sensitive\nSECRET=hunter2";
    let schema = parse(content);
    assert!(schema.entries[0].annotations.sensitive);
}

#[test]
fn parse_annotation_type_port() {
    let content = "# @type=port\nPORT=3000";
    let schema = parse(content);
    assert_eq!(schema.entries[0].annotations.env_type, Some(EnvType::Port));
}

#[test]
fn parse_annotation_type_url() {
    let content = "# @type=url\nAPI_URL=https://example.com";
    let schema = parse(content);
    assert_eq!(schema.entries[0].annotations.env_type, Some(EnvType::Url));
}

#[test]
fn parse_annotation_type_enum() {
    let content = "# @type=enum(dev,staging,prod)\nENV=dev";
    let schema = parse(content);
    assert_eq!(
        schema.entries[0].annotations.env_type,
        Some(EnvType::Enum(vec![
            "dev".to_owned(),
            "staging".to_owned(),
            "prod".to_owned()
        ]))
    );
}

#[test]
fn parse_annotation_type_string() {
    let content = "# @type=string\nNAME=hello";
    let schema = parse(content);
    assert_eq!(
        schema.entries[0].annotations.env_type,
        Some(EnvType::String {
            min_len: None,
            max_len: None
        })
    );
}

#[test]
fn parse_annotation_type_string_with_constraints_and_pattern() {
    let content = "# @type=string @min=3 @max=8 @pattern=^[a-z]+$\nNAME=hello";
    let schema = parse(content);
    assert_eq!(
        schema.entries[0].annotations.env_type,
        Some(EnvType::String {
            min_len: Some(3),
            max_len: Some(8)
        })
    );
    assert_eq!(
        schema.entries[0].annotations.pattern.as_deref(),
        Some("^[a-z]+$")
    );
}

#[test]
fn parse_annotation_min_before_type_string() {
    let content = "# @min=2 @max=5 @type=string\nNAME=hello";
    let schema = parse(content);
    assert_eq!(
        schema.entries[0].annotations.env_type,
        Some(EnvType::String {
            min_len: Some(2),
            max_len: Some(5)
        })
    );
}

#[test]
fn parse_combined_annotations() {
    let content = "# @type=port @required @sensitive\nDB_PORT=5432";
    let schema = parse(content);
    let ann = &schema.entries[0].annotations;
    assert_eq!(ann.env_type, Some(EnvType::Port));
    assert_eq!(ann.required, Some(true));
    assert!(ann.sensitive);
}

#[test]
fn parse_annotation_with_description() {
    let content = "# The server port\n# @type=port @required\nPORT=3000";
    let schema = parse(content);
    assert_eq!(
        schema.entries[0].annotations.description,
        Some("The server port".to_owned())
    );
    assert_eq!(schema.entries[0].annotations.env_type, Some(EnvType::Port));
}

#[test]
fn parse_exec_value() {
    let content = "SECRET=exec('op read \"op://vault/item\"')";
    let schema = parse(content);
    assert_eq!(
        schema.entries[0].value,
        Some(EnvValueExpr::Exec("op read \"op://vault/item\"".to_owned()))
    );
}

#[test]
fn parse_exec_double_quoted() {
    let content = "SECRET=exec(\"echo hello\")";
    let schema = parse(content);
    assert_eq!(
        schema.entries[0].value,
        Some(EnvValueExpr::Exec("echo hello".to_owned()))
    );
}

#[test]
fn parse_env_ref() {
    let content = "BASE_URL=env('API_HOST')";
    let schema = parse(content);
    assert_eq!(
        schema.entries[0].value,
        Some(EnvValueExpr::EnvRef("API_HOST".to_owned()))
    );
}

#[test]
fn parse_template_value() {
    let content = "API_URL=https://${HOST}:${PORT}/api";
    let schema = parse(content);
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
}

#[test]
fn parse_template_single_var() {
    let content = "ALIAS=${OTHER_VAR}";
    let schema = parse(content);
    assert_eq!(
        schema.entries[0].value,
        Some(EnvValueExpr::Template(vec![TemplatePart::VarRef(
            "OTHER_VAR".to_owned()
        )]))
    );
}

#[test]
fn parse_error_invalid_key() {
    let err = parse_err("123BAD=value");
    assert!(err.contains("invalid variable name"), "got: {err}");
}

#[test]
fn parse_error_empty_key() {
    let err = parse_err("=value");
    assert!(err.contains("empty variable name"), "got: {err}");
}

#[test]
fn parse_error_no_equals() {
    let err = parse_err("NOEQUALS");
    assert!(err.contains("expected KEY=VALUE"), "got: {err}");
}

#[test]
fn parse_error_unclosed_template() {
    let err = parse_err("URL=https://${HOST/path");
    assert!(err.contains("unclosed"), "got: {err}");
}

#[test]
fn parse_error_empty_template_var() {
    let err = parse_err("URL=prefix${}suffix");
    assert!(err.contains("empty variable reference"), "got: {err}");
}

#[test]
fn parse_error_unknown_annotation() {
    let err = parse_err("# @bogus\nPORT=3000");
    assert!(err.contains("unknown annotation"), "got: {err}");
}

#[test]
fn parse_error_unknown_type() {
    let err = parse_err("# @type=integer\nPORT=3000");
    assert!(err.contains("unknown type"), "got: {err}");
}

#[test]
fn parse_error_string_constraint_requires_integer() {
    let err = parse_err("# @type=string @min=abc\nNAME=hello");
    assert!(
        err.contains("@min requires a non-negative integer"),
        "got: {err}"
    );
}

#[test]
fn parse_error_string_constraints_require_string_type() {
    let err = parse_err("# @type=port @min=2\nPORT=3000");
    assert!(
        err.contains("@min`/`@max` require `@type=string`"),
        "got: {err}"
    );
}

#[test]
fn parse_error_string_min_cannot_exceed_max() {
    let err = parse_err("# @type=string @min=8 @max=3\nNAME=hello");
    assert!(err.contains("cannot exceed"), "got: {err}");
}

#[test]
fn parse_error_invalid_pattern_regex() {
    let err = parse_err("# @pattern=[a-\nNAME=hello");
    assert!(err.contains("invalid `@pattern` regex"), "got: {err}");
}

#[test]
fn parse_empty_schema() {
    let schema = parse("");
    assert!(schema.entries.is_empty());
}

#[test]
fn parse_comments_only() {
    let schema = parse("# just a comment\n# nothing else");
    assert!(schema.entries.is_empty());
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
