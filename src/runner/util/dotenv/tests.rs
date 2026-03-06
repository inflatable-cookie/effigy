use super::parse_dotenv_entries;

#[test]
fn parse_dotenv_entries_supports_comments_export_and_quotes() {
    let parsed = parse_dotenv_entries(
        r#"
# ignored comment
FOO=bar
export BAR = "quoted value"
BAZ='single quoted'
"#,
    );
    assert_eq!(parsed.get("FOO").map(String::as_str), Some("bar"));
    assert_eq!(parsed.get("BAR").map(String::as_str), Some("quoted value"));
    assert_eq!(parsed.get("BAZ").map(String::as_str), Some("single quoted"));
}

#[test]
fn parse_dotenv_entries_ignores_invalid_lines_and_empty_keys() {
    let parsed = parse_dotenv_entries(
        r#"
NO_EQUALS
=empty_key
KEY_WITH_EQUALS=a=b=c
valid = value
"#,
    );
    assert!(!parsed.contains_key("NO_EQUALS"));
    assert!(!parsed.contains_key(""));
    assert_eq!(
        parsed.get("KEY_WITH_EQUALS").map(String::as_str),
        Some("a=b=c")
    );
    assert_eq!(parsed.get("valid").map(String::as_str), Some("value"));
}

#[test]
fn parse_dotenv_entries_keeps_unmatched_quotes_literal() {
    let parsed = parse_dotenv_entries(
        r#"
UNMATCHED_DQ="value
UNMATCHED_SQ='value
"#,
    );
    assert_eq!(
        parsed.get("UNMATCHED_DQ").map(String::as_str),
        Some("\"value")
    );
    assert_eq!(
        parsed.get("UNMATCHED_SQ").map(String::as_str),
        Some("'value")
    );
}
