use effigy_manifest::TASK_MANIFEST_FILE;
use toml::{map::Map as TomlMap, Value};

use crate::error::CodeGraphError;
use crate::extractor::SourceFile;

pub(super) fn parse_manifest_value(file: &SourceFile) -> Result<Value, CodeGraphError> {
    match toml::from_str(&file.content) {
        Ok(parsed) => Ok(parsed),
        Err(raw_error) if contains_template_markers(&file.content) => {
            let sanitized = sanitize_template_toml(&file.content);
            match toml::from_str(&sanitized) {
                Ok(parsed) => Ok(parsed),
                Err(sanitized_error) => {
                    let fallback = lossy_template_manifest_table(&sanitized);
                    if fallback.is_empty() {
                        Err(CodeGraphError::validation(format!(
                            "failed to parse TOML {}: raw parse error: {raw_error}; sanitized parse error: {sanitized_error}",
                            file.relative_path
                        )))
                    } else {
                        Ok(Value::Table(fallback))
                    }
                }
            }
        }
        Err(error) => Err(CodeGraphError::validation(format!(
            "failed to parse TOML {}: {error}",
            file.relative_path
        ))),
    }
}

fn contains_template_markers(content: &str) -> bool {
    content.contains("{{") || content.contains("{%") || content.contains("{#")
}

fn sanitize_template_toml(content: &str) -> String {
    let mut sanitized = String::with_capacity(content.len());
    let mut chars = content.chars().peekable();
    let mut in_basic_string = false;
    let mut in_literal_string = false;

    while let Some(ch) = chars.next() {
        if in_basic_string {
            if ch == '{' {
                match chars.peek().copied() {
                    Some('{') => {
                        chars.next();
                        consume_until_double_close(&mut chars, '}');
                        sanitized.push_str("template");
                        continue;
                    }
                    Some('%') => {
                        chars.next();
                        consume_until_double_close(&mut chars, '%');
                        continue;
                    }
                    Some('#') => {
                        chars.next();
                        consume_until_double_close(&mut chars, '#');
                        continue;
                    }
                    _ => {}
                }
            }
            sanitized.push(ch);
            if ch == '\\' {
                if let Some(escaped) = chars.next() {
                    sanitized.push(escaped);
                }
                continue;
            }
            if ch == '"' {
                in_basic_string = false;
            }
            continue;
        }

        if in_literal_string {
            if ch == '{' {
                match chars.peek().copied() {
                    Some('{') => {
                        chars.next();
                        consume_until_double_close(&mut chars, '}');
                        sanitized.push_str("template");
                        continue;
                    }
                    Some('%') => {
                        chars.next();
                        consume_until_double_close(&mut chars, '%');
                        continue;
                    }
                    Some('#') => {
                        chars.next();
                        consume_until_double_close(&mut chars, '#');
                        continue;
                    }
                    _ => {}
                }
            }
            sanitized.push(ch);
            if ch == '\'' {
                in_literal_string = false;
            }
            continue;
        }

        match ch {
            '"' => {
                in_basic_string = true;
                sanitized.push(ch);
            }
            '\'' => {
                in_literal_string = true;
                sanitized.push(ch);
            }
            '{' => match chars.peek().copied() {
                Some('{') => {
                    chars.next();
                    consume_until_double_close(&mut chars, '}');
                    sanitized.push_str("\"template\"");
                }
                Some('%') => {
                    chars.next();
                    consume_until_double_close(&mut chars, '%');
                }
                Some('#') => {
                    chars.next();
                    consume_until_double_close(&mut chars, '#');
                }
                _ => sanitized.push(ch),
            },
            _ => sanitized.push(ch),
        }
    }

    sanitized
}

fn consume_until_double_close(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, marker: char) {
    while let Some(ch) = chars.next() {
        if ch == marker && chars.peek().is_some_and(|next| *next == '}') {
            chars.next();
            break;
        }
    }
}

fn lossy_template_manifest_table(content: &str) -> TomlMap<String, Value> {
    let mut root = TomlMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(path) = parse_table_header(trimmed) {
            insert_section_path(&mut root, &path);
            continue;
        }
        if let Some((key, _)) = trimmed.split_once('=') {
            let key = key.trim();
            if is_bare_toml_key(key) {
                root.entry(key.to_owned())
                    .or_insert_with(|| Value::String("template".to_owned()));
            }
        }
    }
    root
}

fn parse_table_header(line: &str) -> Option<Vec<String>> {
    let bracket_count = if line.starts_with("[[") {
        2
    } else if line.starts_with('[') {
        1
    } else {
        0
    };
    if bracket_count == 0 {
        return None;
    }
    let closing = if bracket_count == 2 { "]]" } else { "]" };
    let inner = line
        .strip_prefix(if bracket_count == 2 { "[[" } else { "[" })?
        .split_once(closing)?
        .0
        .trim();
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in inner.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            '.' if !in_quotes => {
                push_header_part(&mut parts, &mut current);
            }
            _ => current.push(ch),
        }
    }
    push_header_part(&mut parts, &mut current);
    (!parts.is_empty()).then_some(parts)
}

fn push_header_part(parts: &mut Vec<String>, current: &mut String) {
    let normalized = current.trim().trim_matches('"').trim_matches('\'').trim();
    if !normalized.is_empty() {
        parts.push(normalized.to_owned());
    }
    current.clear();
}

fn insert_section_path(root: &mut TomlMap<String, Value>, path: &[String]) {
    if path.is_empty() {
        return;
    }
    let mut current = root;
    for segment in path {
        let entry = current
            .entry(segment.clone())
            .or_insert_with(|| Value::Table(TomlMap::new()));
        if !entry.is_table() {
            *entry = Value::Table(TomlMap::new());
        }
        current = entry.as_table_mut().expect("table inserted");
    }
}

fn is_bare_toml_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
}

pub(super) fn is_named_effigy_manifest(relative_path: &str) -> bool {
    relative_path == TASK_MANIFEST_FILE
        || relative_path.ends_with(&format!("/{TASK_MANIFEST_FILE}"))
        || relative_path.ends_with(".effigy.toml")
}

pub(super) fn is_bundle_descriptor_path(relative_path: &str) -> bool {
    relative_path == "bundle.toml" || relative_path.ends_with("/bundle.toml")
}

pub(super) fn looks_like_effigy_manifest(table: &TomlMap<String, Value>) -> bool {
    table.keys().any(|key| {
        matches!(
            key.as_str(),
            "manifest"
                | "catalog"
                | "bundle"
                | "defer"
                | "env"
                | "data"
                | "state"
                | "deploy"
                | "test"
                | "package_manager"
                | "scan"
                | "shell"
                | "env_schema"
                | "secrets"
                | "docs_policy"
                | "task_defaults"
                | "bootstrap"
                | "isolation"
                | "containers"
                | "systems"
                | "distribution"
                | "release"
                | "demos"
                | "tasks"
        )
    })
}
