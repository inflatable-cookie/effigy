use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::error::CodeGraphError;
use crate::model::{FileRecord, SourceSpan, SymbolRecord};
use crate::storage::GraphStore;
use crate::support::span_from_bytes;

use super::profile::{FileRole, RequestIntent};

#[derive(Debug, Clone)]
pub(super) struct ExploreExcerptSection {
    pub(super) text: String,
    pub(super) truncated: bool,
    pub(super) section_kind: String,
    pub(super) completeness: String,
}

#[derive(Debug, Clone)]
pub(super) struct SourceEvidence {
    pub(super) tokens: BTreeSet<String>,
    pub(super) score: i64,
    pub(super) reasons: Vec<String>,
}

pub(super) fn indexed_source_matches<'a>(
    store: &GraphStore,
    tokens: &[String],
    allowed_file_ids: impl IntoIterator<Item = &'a str>,
) -> Result<BTreeMap<String, BTreeSet<String>>, CodeGraphError> {
    let allowed = allowed_file_ids
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let mut matches = BTreeMap::<String, BTreeSet<String>>::new();
    for token in tokens {
        for hit in store.source_search(token, allowed.len().max(1))? {
            if !allowed.contains(hit.file_id.as_str()) {
                continue;
            }
            matches
                .entry(hit.file_id)
                .or_default()
                .insert(token.clone());
        }
    }
    Ok(matches)
}

pub(super) fn indexed_source_evidence(
    matched_tokens: Option<&BTreeSet<String>>,
    role: FileRole,
    intent: RequestIntent,
) -> SourceEvidence {
    let matched_tokens = matched_tokens.cloned().unwrap_or_default();
    if matched_tokens.is_empty() || role == FileRole::Generated {
        return SourceEvidence {
            tokens: BTreeSet::new(),
            score: 0,
            reasons: Vec::new(),
        };
    }
    let score_per_token = match (intent, role) {
        (RequestIntent::Implementation, FileRole::Implementation) => 2,
        (RequestIntent::Implementation, FileRole::Config | FileRole::Test) => 1,
        (RequestIntent::Implementation, FileRole::Docs | FileRole::Planning) => 0,
        (RequestIntent::Docs, FileRole::Docs | FileRole::Planning) => 2,
        (_, FileRole::Implementation) => 2,
        (_, FileRole::Config | FileRole::Test | FileRole::Docs) => 1,
        (_, FileRole::Planning | FileRole::Fixture | FileRole::Generated) => 0,
    };
    if score_per_token == 0 {
        return SourceEvidence {
            tokens: BTreeSet::new(),
            score: 0,
            reasons: Vec::new(),
        };
    }
    let reasons = matched_tokens
        .iter()
        .map(|token| format!("indexed source contains `{token}`"))
        .collect::<Vec<_>>();
    let scored_tokens = matched_tokens.len().min(5);
    SourceEvidence {
        tokens: matched_tokens,
        score: (scored_tokens as i64) * score_per_token,
        reasons,
    }
}

pub(super) fn indexed_source_evidence_span(
    repo_root: &Path,
    file: &FileRecord,
    tokens: &BTreeSet<String>,
    role: FileRole,
    intent: RequestIntent,
) -> Option<SourceSpan> {
    if tokens.is_empty() {
        return None;
    }
    let content = fs::read_to_string(repo_root.join(&file.path)).ok()?;
    let first_token = tokens.iter().next()?;
    let (start, end) = source_token_match(&content, first_token, intent, role)?;
    Some(span_from_bytes(&content, start, end))
}

fn source_token_match(
    content: &str,
    token: &str,
    intent: RequestIntent,
    role: FileRole,
) -> Option<(usize, usize)> {
    let skip_comment_only_lines =
        intent == RequestIntent::Implementation && role == FileRole::Implementation;
    let token = token.to_ascii_lowercase();
    let mut offset = 0usize;
    for line in content.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if skip_comment_only_lines
            && (trimmed.starts_with("//")
                || trimmed.starts_with("/*")
                || trimmed.starts_with('*')
                || trimmed.starts_with('#'))
        {
            offset += line.len();
            continue;
        }
        if let Some(index) = line.to_ascii_lowercase().find(&token) {
            let start = offset + index;
            return Some((start, start + token.len()));
        }
        offset += line.len();
    }
    None
}

pub(super) fn strongest_symbol_span(
    symbol_hits: &[(SymbolRecord, BTreeSet<String>, Vec<String>)],
) -> Option<SourceSpan> {
    symbol_hits
        .iter()
        .max_by(|left, right| {
            left.1
                .len()
                .cmp(&right.1.len())
                .then_with(|| right.0.span.start.byte.cmp(&left.0.span.start.byte))
        })
        .map(|(symbol, _, _)| symbol.span.clone())
}

pub(super) fn file_snippet(
    repo_root: &Path,
    file: &FileRecord,
    evidence_span: Option<&SourceSpan>,
    remaining_bytes: usize,
) -> Option<(String, bool)> {
    if remaining_bytes == 0 {
        return None;
    }
    let content = fs::read_to_string(repo_root.join(&file.path)).ok()?;
    let limit = remaining_bytes.min(240);
    if let Some(span) = evidence_span {
        return bounded_snippet(
            &content,
            span.start.byte as usize,
            span.end.byte as usize,
            limit,
        );
    }
    bounded_snippet(&content, 0, content.len(), limit)
}

pub(super) fn symbol_snippet(
    repo_root: &Path,
    symbol: &SymbolRecord,
    remaining_bytes: usize,
) -> Option<(String, bool)> {
    if remaining_bytes == 0 {
        return None;
    }
    let content = fs::read_to_string(repo_root.join(&symbol.provenance.source_path)).ok()?;
    let limit = remaining_bytes.min(240);
    bounded_snippet(
        &content,
        symbol.span.start.byte as usize,
        symbol.span.end.byte as usize,
        limit,
    )
}

fn bounded_snippet(
    content: &str,
    start_byte: usize,
    end_byte: usize,
    limit: usize,
) -> Option<(String, bool)> {
    if limit == 0 || content.is_empty() {
        return None;
    }
    let start = start_byte.min(content.len());
    let mut end = end_byte.min(content.len()).max(start);
    while end > start && !content.is_char_boundary(end) {
        end -= 1;
    }
    let slice = content.get(start..end).unwrap_or("");
    let mut snippet = slice.trim().to_owned();
    let truncated = snippet.len() > limit;
    if truncated {
        // `String::truncate` panics unless the index is a char boundary, and
        // `limit - 3` lands mid-codepoint whenever a snippet contains
        // multi-byte text — an em-dash in a doc comment is enough. The slice
        // bounds above are already walked back for this reason; this one was
        // not, so a Unicode-bearing result aborted the whole query instead of
        // returning an envelope.
        // The ellipsis has to fit inside the limit too. `limit - 3` saturates
        // to 0 for a limit under 4, which produced a three-byte "..." for a
        // one-byte budget — a bound the function's own name promises to keep.
        let ellipsis = if limit >= 4 { "..." } else { "" };
        let mut cut = limit.saturating_sub(ellipsis.len());
        while cut > 0 && !snippet.is_char_boundary(cut) {
            cut -= 1;
        }
        snippet.truncate(cut);
        snippet.push_str(ellipsis);
    }
    if snippet.is_empty() {
        None
    } else {
        Some((snippet, truncated))
    }
}

fn expanded_bounded_snippet(
    content: &str,
    start_byte: usize,
    end_byte: usize,
    limit: usize,
) -> Option<(String, bool)> {
    if limit == 0 || content.is_empty() {
        return None;
    }
    let start = start_byte.min(content.len());
    let end = end_byte.min(content.len()).max(start);
    let before_budget = limit / 3;
    let after_budget = limit.saturating_sub(before_budget);
    let raw_start = start.saturating_sub(before_budget);
    let raw_end = end.saturating_add(after_budget).min(content.len());
    let snippet_start = line_start_at_or_before(content, raw_start);
    let snippet_end = line_end_at_or_after(content, raw_end);
    bounded_snippet(content, snippet_start, snippet_end, limit)
}

pub(super) fn sectioned_snippet(
    content: &str,
    language_id: Option<&str>,
    role: &str,
    start_byte: usize,
    end_byte: usize,
    limit: usize,
) -> Option<ExploreExcerptSection> {
    if matches!(language_id, Some("markdown")) {
        if let Some((text, truncated)) =
            markdown_heading_section_snippet(content, start_byte, limit)
        {
            return Some(ExploreExcerptSection {
                text,
                truncated,
                section_kind: "heading-section".to_owned(),
                completeness: if truncated {
                    "truncated-section".to_owned()
                } else {
                    "complete-section".to_owned()
                },
            });
        }
    }
    if matches!(language_id, Some("python")) && (role == "symbol" || role == "file") {
        if let Some((text, truncated)) = python_block_section_snippet(content, start_byte, limit) {
            return Some(ExploreExcerptSection {
                text,
                truncated,
                section_kind: "python-block".to_owned(),
                completeness: if truncated {
                    "truncated-section".to_owned()
                } else {
                    "complete-section".to_owned()
                },
            });
        }
    }
    expanded_bounded_snippet(content, start_byte, end_byte, limit).map(|(text, truncated)| {
        ExploreExcerptSection {
            text,
            truncated,
            section_kind: "context-window".to_owned(),
            completeness: "surrounding-context".to_owned(),
        }
    })
}

fn markdown_heading_section_snippet(
    content: &str,
    start_byte: usize,
    limit: usize,
) -> Option<(String, bool)> {
    let lines = content.split_inclusive('\n').collect::<Vec<_>>();
    let starts = line_start_offsets(&lines);
    let line_index = line_index_for_byte(&starts, start_byte)?;
    let heading_index = (0..=line_index)
        .rev()
        .find(|index| markdown_heading_level(lines[*index]).is_some())?;
    let heading_level = markdown_heading_level(lines[heading_index])?;
    let section_start = starts[heading_index];
    let mut section_end = content.len();
    for index in (heading_index + 1)..lines.len() {
        if let Some(level) = markdown_heading_level(lines[index]) {
            if level <= heading_level {
                section_end = starts[index];
                break;
            }
        }
    }
    bounded_snippet(content, section_start, section_end, limit)
}

fn markdown_heading_level(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|ch| *ch == '#').count();
    (hashes > 0 && trimmed.chars().nth(hashes) == Some(' ')).then_some(hashes)
}

fn python_block_section_snippet(
    content: &str,
    start_byte: usize,
    limit: usize,
) -> Option<(String, bool)> {
    let lines = content.split_inclusive('\n').collect::<Vec<_>>();
    let starts = line_start_offsets(&lines);
    let line_index = line_index_for_byte(&starts, start_byte)?;
    let definition_index = (0..=line_index).rev().find(|index| {
        let trimmed = lines[*index].trim_start();
        trimmed.starts_with("def ")
            || trimmed.starts_with("class ")
            || trimmed.starts_with("async def ")
    })?;
    let mut section_start_index = definition_index;
    while section_start_index > 0 && lines[section_start_index - 1].trim_start().starts_with('@') {
        section_start_index -= 1;
    }
    let definition_indent = leading_space_count(lines[definition_index]);
    let mut section_end = content.len();
    let mut saw_body = false;
    for index in (definition_index + 1)..lines.len() {
        let line = lines[index];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let indent = leading_space_count(line);
        if indent > definition_indent {
            saw_body = true;
            continue;
        }
        if saw_body && !line.trim_start().starts_with('#') {
            section_end = starts[index];
            break;
        }
    }
    bounded_snippet(content, starts[section_start_index], section_end, limit)
}

fn line_start_offsets(lines: &[&str]) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(lines.len());
    let mut total = 0usize;
    for line in lines {
        offsets.push(total);
        total += line.len();
    }
    offsets
}

fn line_index_for_byte(starts: &[usize], target: usize) -> Option<usize> {
    if starts.is_empty() {
        return None;
    }
    match starts.binary_search(&target) {
        Ok(index) => Some(index),
        Err(0) => Some(0),
        Err(index) => Some(index - 1),
    }
}

fn leading_space_count(line: &str) -> usize {
    line.chars()
        .take_while(|ch| *ch == ' ' || *ch == '\t')
        .count()
}

fn line_start_at_or_before(content: &str, index: usize) -> usize {
    let mut start = index.min(content.len());
    while start > 0 && !content.is_char_boundary(start) {
        start -= 1;
    }
    content[..start]
        .rfind('\n')
        .map(|position| position + 1)
        .unwrap_or(0)
}

fn line_end_at_or_after(content: &str, index: usize) -> usize {
    let mut end = index.min(content.len());
    while end < content.len() && !content.is_char_boundary(end) {
        end += 1;
    }
    content[end..]
        .find('\n')
        .map(|position| end + position)
        .unwrap_or(content.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The panic this guards: `limit - 3` landing inside a multi-byte
    /// character. An em-dash occupies three bytes, so a snippet containing one
    /// aborts the query for any limit whose cut point falls inside it.
    #[test]
    fn bounded_snippet_truncates_on_a_character_boundary() {
        let content = "focus ring — accent, and a long tail to force truncation";
        for limit in 1..content.len() {
            let Some((snippet, truncated)) = bounded_snippet(content, 0, content.len(), limit)
            else {
                continue;
            };
            assert!(
                snippet.is_char_boundary(snippet.len()),
                "limit {limit} produced a snippet split mid-character"
            );
            if truncated {
                assert!(
                    limit < 4 || snippet.ends_with("..."),
                    "limit {limit} lost its ellipsis"
                );
                assert!(
                    snippet.len() <= limit,
                    "limit {limit} produced {} bytes",
                    snippet.len()
                );
            }
        }
    }

    /// Multi-byte content that is *not* truncated must survive untouched.
    #[test]
    fn bounded_snippet_leaves_short_unicode_alone() {
        let content = "café —";
        let (snippet, truncated) = bounded_snippet(content, 0, content.len(), 512)
            .expect("short content still yields a snippet");
        assert_eq!(snippet, content);
        assert!(!truncated);
    }
}
