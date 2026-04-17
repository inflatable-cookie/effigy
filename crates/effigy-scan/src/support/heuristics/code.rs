use std::ffi::OsStr;
use std::path::Path;

use super::strings::mask_string_literals;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedCodeLine {
    pub line_number: usize,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommentRatioCounts {
    pub code_lines: usize,
    pub comment_lines: usize,
}

pub fn count_code_lines(path: &Path, contents: &str) -> usize {
    normalized_code_lines(path, contents).len()
}

pub fn normalized_code_lines(path: &Path, contents: &str) -> Vec<NormalizedCodeLine> {
    let ext = path.extension().and_then(OsStr::to_str).unwrap_or_default();
    match ext {
        "py" | "rb" | "sh" | "toml" | "yaml" | "yml" | "zsh" => {
            collect_comment_filtered_lines(contents, Some("#"), None)
        }
        "sql" => collect_comment_filtered_lines(contents, Some("--"), Some(("/*", "*/"))),
        "html" | "xml" | "vue" => {
            collect_comment_filtered_lines(contents, None, Some(("<!--", "-->")))
        }
        "c" | "cc" | "cpp" | "cs" | "css" | "go" | "h" | "hpp" | "java" | "js" | "jsx" | "kt"
        | "kts" | "m" | "mm" | "php" | "rs" | "scala" | "sc" | "swift" | "ts" | "tsx" => {
            collect_comment_filtered_lines(contents, Some("//"), Some(("/*", "*/")))
        }
        _ => contents
            .lines()
            .enumerate()
            .filter_map(|(line_number, line)| {
                let normalized = normalize_code_fragment(line);
                (!normalized.is_empty()).then(|| NormalizedCodeLine {
                    line_number: line_number + 1,
                    text: normalized,
                })
            })
            .collect(),
    }
}

pub fn comment_ratio_counts(path: &Path, contents: &str) -> CommentRatioCounts {
    let ext = path.extension().and_then(OsStr::to_str).unwrap_or_default();
    match ext {
        "py" | "rb" | "sh" | "toml" | "yaml" | "yml" | "zsh" => {
            count_code_and_comment_lines(contents, Some("#"), None)
        }
        "sql" => count_code_and_comment_lines(contents, Some("--"), Some(("/*", "*/"))),
        "html" | "xml" | "vue" => {
            count_code_and_comment_lines(contents, None, Some(("<!--", "-->")))
        }
        "c" | "cc" | "cpp" | "cs" | "css" | "go" | "h" | "hpp" | "java" | "js" | "jsx" | "kt"
        | "kts" | "m" | "mm" | "php" | "rs" | "scala" | "sc" | "swift" | "ts" | "tsx" => {
            count_code_and_comment_lines(contents, Some("//"), Some(("/*", "*/")))
        }
        _ => CommentRatioCounts {
            code_lines: contents
                .lines()
                .filter(|line| !normalize_code_fragment(line).is_empty())
                .count(),
            comment_lines: 0,
        },
    }
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

fn collect_comment_filtered_lines(
    contents: &str,
    line_comment: Option<&str>,
    block_comment: Option<(&str, &str)>,
) -> Vec<NormalizedCodeLine> {
    let mut in_block_comment = false;
    let mut lines = Vec::new();
    for (line_number, raw_line) in contents.lines().enumerate() {
        let masked_line = mask_string_literals(raw_line);
        let mut i = 0usize;
        let mut fragments = String::new();
        while i < masked_line.len() {
            if in_block_comment {
                if let Some((_, block_end)) = block_comment {
                    if masked_line[i..].starts_with(block_end) {
                        in_block_comment = false;
                        i += block_end.len();
                        continue;
                    }
                }
                let ch = masked_line[i..]
                    .chars()
                    .next()
                    .expect("unicode-safe block comment advance");
                i += ch.len_utf8();
                continue;
            }
            if let Some((block_start, _)) = block_comment {
                if masked_line[i..].starts_with(block_start) {
                    in_block_comment = true;
                    i += block_start.len();
                    continue;
                }
            }
            if let Some(line_prefix) = line_comment {
                if masked_line[i..].starts_with(line_prefix) {
                    break;
                }
            }
            let ch = masked_line[i..]
                .chars()
                .next()
                .expect("unicode-safe line advance");
            fragments.push(ch);
            i += ch.len_utf8();
        }
        let normalized = normalize_code_fragment(&fragments);
        if !normalized.is_empty() {
            lines.push(NormalizedCodeLine {
                line_number: line_number + 1,
                text: normalized,
            });
        }
    }
    lines
}

fn count_code_and_comment_lines(
    contents: &str,
    line_comment: Option<&str>,
    block_comment: Option<(&str, &str)>,
) -> CommentRatioCounts {
    let mut in_block_comment = false;
    let mut code_lines = 0usize;
    let mut comment_lines = 0usize;

    for raw_line in contents.lines() {
        let masked_line = mask_string_literals(raw_line);
        let mut i = 0usize;
        let mut fragments = String::new();
        let mut saw_comment = false;
        while i < masked_line.len() {
            if in_block_comment {
                saw_comment = true;
                if let Some((_, block_end)) = block_comment {
                    if masked_line[i..].starts_with(block_end) {
                        in_block_comment = false;
                        i += block_end.len();
                        continue;
                    }
                }
                let ch = masked_line[i..]
                    .chars()
                    .next()
                    .expect("unicode-safe block comment advance");
                i += ch.len_utf8();
                continue;
            }
            if let Some((block_start, _)) = block_comment {
                if masked_line[i..].starts_with(block_start) {
                    in_block_comment = true;
                    saw_comment = true;
                    i += block_start.len();
                    continue;
                }
            }
            if let Some(line_prefix) = line_comment {
                if masked_line[i..].starts_with(line_prefix) {
                    saw_comment = true;
                    break;
                }
            }
            let ch = masked_line[i..]
                .chars()
                .next()
                .expect("unicode-safe line advance");
            fragments.push(ch);
            i += ch.len_utf8();
        }
        let normalized_code = normalize_code_fragment(&fragments);
        if !normalized_code.is_empty() {
            code_lines += 1;
        } else if saw_comment {
            comment_lines += 1;
        }
    }

    CommentRatioCounts {
        code_lines,
        comment_lines,
    }
}

fn normalize_code_fragment(fragment: &str) -> String {
    fragment.split_whitespace().collect::<Vec<&str>>().join(" ")
}
