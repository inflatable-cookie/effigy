//! Minimal dotenv parser used when loading `.env` overrides alongside
//! an `.env.schema`.
//!
//! Moved out of `src/runner/util/dotenv.rs` in batch 241 so
//! `effigy-managed` and the runner's `env_schema_support` adapter can
//! both reach it from a shared crate.

use std::collections::BTreeMap;

pub fn parse_dotenv_entries(src: &str) -> BTreeMap<String, String> {
    let mut entries = BTreeMap::new();
    for raw_line in src.lines() {
        let mut line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(exported) = line.strip_prefix("export ") {
            line = exported.trim_start();
        }
        let Some((key_raw, value_raw)) = line.split_once('=') else {
            continue;
        };
        let key = key_raw.trim();
        if key.is_empty() {
            continue;
        }
        let value = strip_matching_quotes(value_raw.trim());
        entries.insert(key.to_owned(), value.to_owned());
    }
    entries
}

fn strip_matching_quotes(value: &str) -> &str {
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
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
}
