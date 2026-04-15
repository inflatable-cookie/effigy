//! Line-based parser for `.env.schema` files.
//!
//! The parser is a hand-written state machine that processes the file line by
//! line. Comment lines (starting with `#`) are accumulated into an annotation
//! buffer. When a `KEY=VALUE` line is encountered, the buffered comments are
//! parsed for `@annotation` tokens, the value expression is parsed, and a
//! schema entry is emitted. Blank lines reset the annotation buffer.
//!
//! This parser is intentionally simple and does not use parser combinators.
//! The grammar is line-oriented: every non-blank, non-comment line must be a
//! `KEY=VALUE` pair (the value may be empty).

use std::path::Path;

use regex::Regex;

use super::ast::{
    EntryAnnotations, EnvSchema, EnvSchemaEntry, EnvType, EnvValueExpr, TemplatePart,
};
use super::error::EnvSchemaError;

/// Parse the content of a `.env.schema` file into an [`EnvSchema`] AST.
///
/// Lines are processed sequentially. Comment lines (`# ...`) accumulate into
/// an annotation buffer that is consumed by the next `KEY=VALUE` line. Blank
/// lines reset the buffer. Returns an error on invalid syntax.
pub(super) fn parse_env_schema(
    content: &str,
    source_path: &Path,
) -> Result<EnvSchema, EnvSchemaError> {
    let mut entries = Vec::new();
    let mut comment_lines: Vec<&str> = Vec::new();
    let mut first_comment_line: usize = 0;

    for (line_idx, raw_line) in content.lines().enumerate() {
        let line_num = line_idx + 1;
        let trimmed = raw_line.trim();

        if trimmed.is_empty() {
            comment_lines.clear();
            continue;
        }

        if let Some(comment_text) = trimmed.strip_prefix('#') {
            if comment_lines.is_empty() {
                first_comment_line = line_num;
            }
            comment_lines.push(comment_text.trim());
            continue;
        }

        let Some((key_raw, value_raw)) = trimmed.split_once('=') else {
            return Err(EnvSchemaError::Parse {
                path: source_path.to_owned(),
                line: line_num,
                message: format!("expected KEY=VALUE, got: {trimmed}"),
            });
        };

        let key = key_raw.trim();
        if key.is_empty() {
            return Err(EnvSchemaError::Parse {
                path: source_path.to_owned(),
                line: line_num,
                message: "empty variable name".to_owned(),
            });
        }
        if !is_valid_env_key(key) {
            return Err(EnvSchemaError::Parse {
                path: source_path.to_owned(),
                line: line_num,
                message: format!("invalid variable name `{key}` (must be [A-Za-z_][A-Za-z0-9_]*)"),
            });
        }

        let annotations = parse_annotations(&comment_lines, source_path, first_comment_line)?;
        let value = parse_value_expr(value_raw.trim(), source_path, line_num)?;

        entries.push(EnvSchemaEntry {
            key: key.to_owned(),
            value,
            annotations,
            line: line_num,
        });
        comment_lines.clear();
    }

    Ok(EnvSchema {
        entries,
        source_path: source_path.to_owned(),
    })
}

fn is_valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn parse_annotations(
    comment_lines: &[&str],
    source_path: &Path,
    first_line: usize,
) -> Result<EntryAnnotations, EnvSchemaError> {
    let mut annotations = EntryAnnotations::default();
    let mut description_parts: Vec<String> = Vec::new();
    let mut string_min_len = None;
    let mut string_max_len = None;

    for (offset, line) in comment_lines.iter().enumerate() {
        let line_num = first_line + offset;
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let mut has_annotation = false;

        for token in &tokens {
            if let Some(annotation) = token.strip_prefix('@') {
                has_annotation = true;
                parse_single_annotation(
                    annotation,
                    &mut annotations,
                    &mut string_min_len,
                    &mut string_max_len,
                    source_path,
                    line_num,
                )?;
            } else if !has_annotation {
                description_parts.push((*token).to_owned());
            }
        }
    }

    if !description_parts.is_empty() {
        annotations.description = Some(description_parts.join(" "));
    }

    finalize_string_constraints(
        &mut annotations,
        string_min_len,
        string_max_len,
        source_path,
        first_line,
    )?;

    Ok(annotations)
}

fn parse_single_annotation(
    annotation: &str,
    target: &mut EntryAnnotations,
    string_min_len: &mut Option<usize>,
    string_max_len: &mut Option<usize>,
    source_path: &Path,
    line_num: usize,
) -> Result<(), EnvSchemaError> {
    let (key, value) = match annotation.split_once('=') {
        Some((k, v)) => (k, Some(v)),
        None => (annotation, None),
    };

    match key {
        "required" => {
            target.required = Some(true);
        }
        "optional" => {
            target.required = Some(false);
        }
        "sensitive" => {
            target.sensitive = true;
        }
        "type" => {
            if target.env_type.is_some() {
                return Err(EnvSchemaError::Parse {
                    path: source_path.to_owned(),
                    line: line_num,
                    message: "duplicate `@type` annotation".to_owned(),
                });
            }
            let type_value = value.ok_or_else(|| EnvSchemaError::Parse {
                path: source_path.to_owned(),
                line: line_num,
                message: "@type requires a value (e.g., @type=port)".to_owned(),
            })?;
            target.env_type = Some(parse_type_annotation(type_value, source_path, line_num)?);
        }
        "default" => {
            target.required = target.required.or(Some(false));
        }
        "min" => {
            *string_min_len = Some(parse_length_annotation(
                "@min",
                value,
                *string_min_len,
                source_path,
                line_num,
            )?);
        }
        "max" => {
            *string_max_len = Some(parse_length_annotation(
                "@max",
                value,
                *string_max_len,
                source_path,
                line_num,
            )?);
        }
        "pattern" => {
            if target.pattern.is_some() {
                return Err(EnvSchemaError::Parse {
                    path: source_path.to_owned(),
                    line: line_num,
                    message: "duplicate `@pattern` annotation".to_owned(),
                });
            }
            let pattern_value =
                strip_matching_quotes(value.ok_or_else(|| EnvSchemaError::Parse {
                    path: source_path.to_owned(),
                    line: line_num,
                    message: "@pattern requires a value (e.g., @pattern=^https://)".to_owned(),
                })?);
            if pattern_value.is_empty() {
                return Err(EnvSchemaError::Parse {
                    path: source_path.to_owned(),
                    line: line_num,
                    message: "@pattern cannot be empty".to_owned(),
                });
            }
            Regex::new(pattern_value).map_err(|error| EnvSchemaError::Parse {
                path: source_path.to_owned(),
                line: line_num,
                message: format!("invalid `@pattern` regex `{pattern_value}`: {error}"),
            })?;
            target.pattern = Some(pattern_value.to_owned());
        }
        other => {
            return Err(EnvSchemaError::Parse {
                path: source_path.to_owned(),
                line: line_num,
                message: format!("unknown annotation `@{other}`"),
            });
        }
    }

    Ok(())
}

fn parse_length_annotation(
    name: &str,
    value: Option<&str>,
    current: Option<usize>,
    source_path: &Path,
    line_num: usize,
) -> Result<usize, EnvSchemaError> {
    if current.is_some() {
        return Err(EnvSchemaError::Parse {
            path: source_path.to_owned(),
            line: line_num,
            message: format!("duplicate `{name}` annotation"),
        });
    }

    let raw_value = value.ok_or_else(|| EnvSchemaError::Parse {
        path: source_path.to_owned(),
        line: line_num,
        message: format!("{name} requires a numeric value"),
    })?;
    raw_value
        .parse::<usize>()
        .map_err(|_| EnvSchemaError::Parse {
            path: source_path.to_owned(),
            line: line_num,
            message: format!("{name} requires a non-negative integer, got `{raw_value}`"),
        })
}

fn finalize_string_constraints(
    annotations: &mut EntryAnnotations,
    min_len: Option<usize>,
    max_len: Option<usize>,
    source_path: &Path,
    line_num: usize,
) -> Result<(), EnvSchemaError> {
    if min_len.is_none() && max_len.is_none() {
        return Ok(());
    }

    if let (Some(min), Some(max)) = (min_len, max_len) {
        if min > max {
            return Err(EnvSchemaError::Parse {
                path: source_path.to_owned(),
                line: line_num,
                message: format!(
                    "invalid string constraints: `@min={min}` cannot exceed `@max={max}`"
                ),
            });
        }
    }

    match annotations.env_type.take() {
        None | Some(EnvType::String { .. }) => {
            annotations.env_type = Some(EnvType::String { min_len, max_len });
            Ok(())
        }
        Some(other_type) => Err(EnvSchemaError::Parse {
            path: source_path.to_owned(),
            line: line_num,
            message: format!(
                "`@min`/`@max` require `@type=string`, found `{}`",
                render_env_type_name(&other_type)
            ),
        }),
    }
}

fn render_env_type_name(env_type: &EnvType) -> &'static str {
    match env_type {
        EnvType::Port => "port",
        EnvType::Url => "url",
        EnvType::String { .. } => "string",
        EnvType::Enum(_) => "enum",
    }
}

fn parse_type_annotation(
    value: &str,
    source_path: &Path,
    line_num: usize,
) -> Result<EnvType, EnvSchemaError> {
    if value == "port" {
        return Ok(EnvType::Port);
    }
    if value == "url" {
        return Ok(EnvType::Url);
    }
    if value == "string" {
        return Ok(EnvType::String {
            min_len: None,
            max_len: None,
        });
    }
    if let Some(inner) = value
        .strip_prefix("enum(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let variants: Vec<String> = inner.split(',').map(|s| s.trim().to_owned()).collect();
        if variants.is_empty() || variants.iter().any(|v| v.is_empty()) {
            return Err(EnvSchemaError::Parse {
                path: source_path.to_owned(),
                line: line_num,
                message: "enum type must have at least one non-empty variant".to_owned(),
            });
        }
        return Ok(EnvType::Enum(variants));
    }

    Err(EnvSchemaError::Parse {
        path: source_path.to_owned(),
        line: line_num,
        message: format!("unknown type `{value}` (valid: port, url, string, enum(...))"),
    })
}

fn parse_value_expr(
    value: &str,
    source_path: &Path,
    line_num: usize,
) -> Result<Option<EnvValueExpr>, EnvSchemaError> {
    if value.is_empty() {
        return Ok(None);
    }

    let unquoted = strip_matching_quotes(value);

    if let Some(inner) = try_parse_exec(unquoted) {
        return Ok(Some(EnvValueExpr::Exec(inner.to_owned())));
    }

    if let Some(inner) = try_parse_env_ref(unquoted) {
        return Ok(Some(EnvValueExpr::EnvRef(inner.to_owned())));
    }

    if unquoted.contains("${") {
        return Ok(Some(parse_template(unquoted, source_path, line_num)?));
    }

    Ok(Some(EnvValueExpr::Literal(unquoted.to_owned())))
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

fn try_parse_exec(value: &str) -> Option<&str> {
    let inner = value.strip_prefix("exec(")?;
    let inner = inner.strip_suffix(')')?;
    let inner = inner.trim();
    let inner = strip_matching_quotes(inner);
    Some(inner)
}

fn try_parse_env_ref(value: &str) -> Option<&str> {
    let inner = value.strip_prefix("env(")?;
    let inner = inner.strip_suffix(')')?;
    let inner = inner.trim();
    let inner = strip_matching_quotes(inner);
    Some(inner)
}

fn parse_template(
    value: &str,
    source_path: &Path,
    line_num: usize,
) -> Result<EnvValueExpr, EnvSchemaError> {
    let mut parts = Vec::new();
    let mut current_literal = String::new();
    let mut chars = value.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            if !current_literal.is_empty() {
                parts.push(TemplatePart::Literal(std::mem::take(&mut current_literal)));
            }
            let mut var_name = String::new();
            loop {
                match chars.next() {
                    Some('}') => break,
                    Some(c) => var_name.push(c),
                    None => {
                        return Err(EnvSchemaError::Parse {
                            path: source_path.to_owned(),
                            line: line_num,
                            message: "unclosed `${` in template expression".to_owned(),
                        });
                    }
                }
            }
            if var_name.is_empty() {
                return Err(EnvSchemaError::Parse {
                    path: source_path.to_owned(),
                    line: line_num,
                    message: "empty variable reference `${}`".to_owned(),
                });
            }
            parts.push(TemplatePart::VarRef(var_name));
        } else {
            current_literal.push(ch);
        }
    }
    if !current_literal.is_empty() {
        parts.push(TemplatePart::Literal(current_literal));
    }

    if parts.len() == 1 {
        if let TemplatePart::Literal(lit) = &parts[0] {
            return Ok(EnvValueExpr::Literal(lit.clone()));
        }
    }

    Ok(EnvValueExpr::Template(parts))
}

#[cfg(test)]
#[path = "tests_src/parser/tests.rs"]
mod tests;
