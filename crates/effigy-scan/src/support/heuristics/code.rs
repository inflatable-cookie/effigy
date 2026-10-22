use std::ffi::OsStr;
use std::path::Path;

use super::strings::mask_string_literals;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedCodeLine {
    pub line_number: usize,
    pub text: String,
}

#[derive(Clone, Copy)]
struct CommentSyntax {
    line: Option<&'static str>,
    block: Option<(&'static str, &'static str)>,
}

fn comment_syntax(extension: &str) -> Option<CommentSyntax> {
    match extension {
        "py" | "rb" | "sh" | "toml" | "yaml" | "yml" | "zsh" => Some(CommentSyntax {
            line: Some("#"),
            block: None,
        }),
        "sql" => Some(CommentSyntax {
            line: Some("--"),
            block: Some(("/*", "*/")),
        }),
        "html" | "xml" | "vue" => Some(CommentSyntax {
            line: None,
            block: Some(("<!--", "-->")),
        }),
        "c" | "cc" | "cpp" | "cs" | "css" | "go" | "h" | "hpp" | "java" | "js" | "jsx" | "kt"
        | "kts" | "m" | "mm" | "php" | "rs" | "scala" | "sc" | "swift" | "ts" | "tsx" => {
            Some(CommentSyntax {
                line: Some("//"),
                block: Some(("/*", "*/")),
            })
        }
        _ => None,
    }
}

fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// Counts code-only lines using the file's comment syntax, without allocating a
/// normalized line per source line.
pub fn count_code_lines(path: &Path, contents: &str) -> usize {
    match comment_syntax(&extension_of(path)) {
        Some(syntax) => {
            let mut in_block = false;
            let mut buffer = String::new();
            contents
                .lines()
                .filter(|raw_line| {
                    buffer.clear();
                    strip_comments_into(raw_line, syntax, &mut in_block, &mut buffer);
                    has_code(&buffer)
                })
                .count()
        }
        None => contents.lines().filter(|line| has_code(line)).count(),
    }
}

/// Normalized code lines with comments removed. String-literal contents are
/// preserved so unrelated data tables cannot collapse into the same block.
pub fn normalized_code_lines(path: &Path, contents: &str) -> Vec<NormalizedCodeLine> {
    match comment_syntax(&extension_of(path)) {
        Some(syntax) => {
            let mut in_block = false;
            let mut buffer = String::new();
            let mut lines = Vec::new();
            for (line_number, raw_line) in contents.lines().enumerate() {
                buffer.clear();
                strip_comments_into(raw_line, syntax, &mut in_block, &mut buffer);
                if let Some(text) = normalize_whitespace(&buffer) {
                    lines.push(NormalizedCodeLine {
                        line_number: line_number + 1,
                        text,
                    });
                }
            }
            lines
        }
        None => contents
            .lines()
            .enumerate()
            .filter_map(|(line_number, raw_line)| {
                normalize_whitespace(raw_line).map(|text| NormalizedCodeLine {
                    line_number: line_number + 1,
                    text,
                })
            })
            .collect(),
    }
}

/// A block whose every line is punctuation and string literals carries data,
/// not behavior: tables, fixtures and string lists. Masking literals leaves no
/// identifier behind, so the block is not an actionable duplicate.
pub fn is_literal_only_block(lines: &[NormalizedCodeLine]) -> bool {
    !lines.is_empty()
        && lines.iter().all(|line| {
            !mask_string_literals(&line.text)
                .chars()
                .any(|character| character.is_alphanumeric())
        })
}

pub fn trim_snippet(line: &str, max_chars: usize) -> String {
    let trimmed = line.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_owned();
    }
    let prefix = trimmed
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    format!("{prefix}...")
}

fn strip_comments_into(
    raw_line: &str,
    syntax: CommentSyntax,
    in_block: &mut bool,
    out: &mut String,
) {
    // Masking keeps byte offsets aligned with `raw_line`, so comment markers
    // inside string literals are invisible here while the original characters
    // (literal contents included) are copied through.
    let masked_line = mask_string_literals(raw_line);
    let mut index = 0usize;
    while index < masked_line.len() {
        if *in_block {
            if let Some((_, block_end)) = syntax.block {
                if masked_line[index..].starts_with(block_end) {
                    *in_block = false;
                    index += block_end.len();
                    continue;
                }
            }
            index += next_char_len(&masked_line[index..]);
            continue;
        }
        if let Some((block_start, _)) = syntax.block {
            if masked_line[index..].starts_with(block_start) {
                *in_block = true;
                index += block_start.len();
                continue;
            }
        }
        if let Some(line_prefix) = syntax.line {
            if masked_line[index..].starts_with(line_prefix) {
                break;
            }
        }
        let character = raw_line[index..]
            .chars()
            .next()
            .expect("index always sits on a character boundary");
        out.push(character);
        index += character.len_utf8();
    }
}

fn next_char_len(text: &str) -> usize {
    text.chars().next().map(char::len_utf8).unwrap_or_default()
}

fn has_code(fragment: &str) -> bool {
    fragment.split_whitespace().next().is_some()
}

fn normalize_whitespace(fragment: &str) -> Option<String> {
    let mut parts = fragment.split_whitespace();
    let first = parts.next()?;
    let mut normalized = String::from(first);
    for part in parts {
        normalized.push(' ');
        normalized.push_str(part);
    }
    Some(normalized)
}
