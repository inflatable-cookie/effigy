use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Parser, Tag, TagEnd};

use super::paths::{
    code_fence_language, heading_level_number, local_path_references, resolve_local_path, slugify,
};
use crate::docs_profile::CompiledDocsProfile;
use crate::error::CodeGraphError;
use crate::extractor::{file_graph_id, GraphSink, SourceFile};
use crate::model::{
    Confidence, DiagnosticRecord, DiagnosticSeverity, EdgeRecord, FileRecord, ReferenceRecord,
    SymbolRecord,
};
use crate::support::{full_span, id_fragment, provenance_for_file, span_from_bytes};
use crate::{ExtractorId, GraphId};

const DOC_REL_KIND: &str = "doc-rel";

struct Heading {
    level: u8,
    text: String,
    start_byte: usize,
    heading_end_byte: usize,
}

struct Fence {
    info: String,
    start_byte: usize,
    end_byte: usize,
}

struct Link {
    dest: String,
    span: Range<usize>,
}

struct MarkdownSource<'a> {
    extractor_id: &'a ExtractorId,
    extractor_version: &'a str,
    file: &'a SourceFile,
    file_record: &'a FileRecord,
    file_symbol_id: &'a GraphId,
}

pub(super) fn extract_markdown(
    extractor_id: &ExtractorId,
    extractor_version: &str,
    profile: Option<&CompiledDocsProfile>,
    file: &SourceFile,
    file_record: &FileRecord,
    sink: &mut GraphSink,
) -> Result<(), CodeGraphError> {
    let content = &file.content;
    let file_symbol_id = GraphId::new(format!("symbol:doc:file:{}", file.relative_path))?;
    let source = MarkdownSource {
        extractor_id,
        extractor_version,
        file,
        file_record,
        file_symbol_id: &file_symbol_id,
    };
    let document_kind = profile
        .and_then(|profile| profile.kind_for(&file.relative_path))
        .map(|kind| kind.token.as_str())
        .unwrap_or("document");
    sink.push_symbol(SymbolRecord {
        id: file_symbol_id.clone(),
        kind: document_kind.to_owned(),
        display_name: file.relative_path.clone(),
        canonical_name: file.relative_path.clone(),
        file_id: file_record.id.clone(),
        span: full_span(content),
        provenance: provenance_for_file(
            extractor_id,
            extractor_version,
            file,
            Confidence::Exact,
            Some("document"),
        ),
    });

    let mut headings = Vec::new();
    let mut fences = Vec::new();
    let mut links = Vec::new();
    let mut heading_level: Option<HeadingLevel> = None;
    let mut heading_start = 0usize;
    let mut heading_text = String::new();
    let mut fence_info: Option<String> = None;
    let mut fence_start = 0usize;
    let mut link_dest: Option<String> = None;
    let mut link_start = 0usize;
    let mut emitted_path_refs = BTreeSet::new();
    let mut emitted_link_file_refs = BTreeSet::new();
    let frontmatter = leading_yaml_frontmatter_range(content);

    for (event, range) in Parser::new(content).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                heading_level = Some(level);
                heading_start = range.start;
                heading_text.clear();
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(level) = heading_level.take() {
                    // A complete leading YAML fence can parse as one setext
                    // heading. Keep that block as metadata only.
                    if frontmatter
                        .as_ref()
                        .is_some_and(|block| heading_start < block.end)
                    {
                        heading_text.clear();
                        continue;
                    }
                    headings.push(Heading {
                        level: heading_level_number(level),
                        text: heading_text.trim().to_owned(),
                        start_byte: heading_start,
                        heading_end_byte: range.end,
                    });
                }
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                fence_start = range.start;
                fence_info = Some(match kind {
                    CodeBlockKind::Indented => "text".to_owned(),
                    CodeBlockKind::Fenced(info) => info.to_string(),
                });
            }
            Event::End(TagEnd::CodeBlock) => {
                fences.push(Fence {
                    info: fence_info.take().unwrap_or_else(|| "text".to_owned()),
                    start_byte: fence_start,
                    end_byte: range.end,
                });
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                link_dest = Some(dest_url.to_string());
                link_start = range.start;
            }
            Event::End(TagEnd::Link) => {
                if let Some(dest) = link_dest.take() {
                    links.push(Link {
                        dest,
                        span: link_start..range.end,
                    });
                }
            }
            Event::Text(text) => {
                push_text_and_path_refs(
                    &source,
                    &text,
                    heading_level.is_some(),
                    &mut heading_text,
                    (Confidence::Syntactic, "path-reference"),
                    &mut emitted_path_refs,
                    sink,
                )?;
            }
            Event::Code(text) => {
                push_text_and_path_refs(
                    &source,
                    &text,
                    heading_level.is_some(),
                    &mut heading_text,
                    (Confidence::Exact, "code-path-reference"),
                    &mut emitted_path_refs,
                    sink,
                )?;
            }
            _ => {}
        }
    }

    emit_headings(&source, content, &headings, sink)?;
    emit_fences(&source, &fences, sink)?;
    emit_links(&source, &links, &mut emitted_link_file_refs, sink)?;

    if let Some(profile) = profile {
        if profile.contains_path(&file.relative_path) {
            emit_field_facts(&source, profile, content, &headings, &fences, sink)?;
            emit_typed_relations(&source, profile, content, &headings, &fences, &links, sink)?;
        }
    }

    Ok(())
}

fn push_text_and_path_refs(
    source: &MarkdownSource<'_>,
    text: &str,
    in_heading: bool,
    heading_text: &mut String,
    style: (Confidence, &'static str),
    emitted_path_refs: &mut BTreeSet<String>,
    sink: &mut GraphSink,
) -> Result<(), CodeGraphError> {
    let (confidence, detail) = style;
    let MarkdownSource {
        extractor_id,
        extractor_version,
        file,
        ..
    } = source;
    let file_symbol_id = source.file_symbol_id.clone();
    if in_heading {
        heading_text.push_str(text);
    }
    for path in local_path_references(file, text) {
        if emitted_path_refs.insert(path.clone()) {
            sink.push_edge(EdgeRecord {
                id: GraphId::new(format!(
                    "edge:doc-path-ref:{}:{}",
                    file.relative_path,
                    id_fragment(&path)
                ))?,
                kind: "doc-path-ref".to_owned(),
                from_id: file_symbol_id.clone(),
                to_id: Some(file_graph_id(&path)?),
                unresolved_target: None,
                provenance: provenance_for_file(
                    extractor_id,
                    extractor_version,
                    file,
                    confidence,
                    Some(detail),
                ),
            });
        }
    }
    Ok(())
}

fn emit_headings(
    source: &MarkdownSource<'_>,
    content: &str,
    headings: &[Heading],
    sink: &mut GraphSink,
) -> Result<(), CodeGraphError> {
    let MarkdownSource {
        extractor_id,
        extractor_version,
        file,
        file_record,
        ..
    } = source;
    let file_symbol_id = source.file_symbol_id.clone();
    let mut stack: Vec<(u8, GraphId)> = Vec::new();
    for (index, heading) in headings.iter().enumerate() {
        let heading_text = heading.text.trim();
        if heading_text.is_empty() {
            continue;
        }
        let anchor = slugify(heading_text);
        if anchor.is_empty() {
            continue;
        }
        let end_byte = headings
            .iter()
            .skip(index + 1)
            .find(|next| next.level <= heading.level)
            .map(|next| next.start_byte)
            .unwrap_or(content.len());
        let symbol_id = GraphId::new(format!("symbol:doc:{}:#{}", file.relative_path, anchor))?;
        sink.push_symbol(SymbolRecord {
            id: symbol_id.clone(),
            kind: format!("heading-h{}", heading.level),
            display_name: heading_text.to_owned(),
            canonical_name: format!("{}#{}", file.relative_path, anchor),
            file_id: file_record.id.clone(),
            span: span_from_bytes(
                content,
                heading.start_byte,
                end_byte.max(heading.start_byte),
            ),
            provenance: provenance_for_file(
                extractor_id,
                extractor_version,
                file,
                Confidence::Exact,
                Some("heading"),
            ),
        });
        sink.push_edge(EdgeRecord {
            id: GraphId::new(format!("edge:contains:{file_symbol_id}:{symbol_id}"))?,
            kind: "contains".to_owned(),
            from_id: file_symbol_id.clone(),
            to_id: Some(symbol_id.clone()),
            unresolved_target: None,
            provenance: provenance_for_file(
                extractor_id,
                extractor_version,
                file,
                Confidence::Exact,
                Some("containment"),
            ),
        });
        while stack
            .last()
            .is_some_and(|(level, _)| *level >= heading.level)
        {
            stack.pop();
        }
        if let Some((_, parent_id)) = stack.last() {
            sink.push_edge(EdgeRecord {
                id: GraphId::new(format!("edge:contains:{parent_id}:{symbol_id}"))?,
                kind: "contains".to_owned(),
                from_id: parent_id.clone(),
                to_id: Some(symbol_id.clone()),
                unresolved_target: None,
                provenance: provenance_for_file(
                    extractor_id,
                    extractor_version,
                    file,
                    Confidence::Exact,
                    Some("containment"),
                ),
            });
        }
        stack.push((heading.level, symbol_id));
    }
    Ok(())
}

fn emit_fences(
    source: &MarkdownSource<'_>,
    fences: &[Fence],
    sink: &mut GraphSink,
) -> Result<(), CodeGraphError> {
    let MarkdownSource {
        extractor_id,
        extractor_version,
        file,
        file_record,
        ..
    } = source;
    let file_symbol_id = source.file_symbol_id.clone();
    for (index, fence) in fences.iter().enumerate() {
        let code_fence_index = index + 1;
        let fence_id = GraphId::new(format!(
            "symbol:doc:{}:fence:{}",
            file.relative_path, code_fence_index
        ))?;
        let language = code_fence_language(&fence.info);
        sink.push_symbol(SymbolRecord {
            id: fence_id.clone(),
            kind: "code-fence".to_owned(),
            display_name: language.to_owned(),
            canonical_name: format!("{}::code-fence::{code_fence_index}", file.relative_path),
            file_id: file_record.id.clone(),
            span: full_span(&file.content),
            provenance: provenance_for_file(
                extractor_id,
                extractor_version,
                file,
                Confidence::Exact,
                Some("code-fence"),
            ),
        });
        sink.push_edge(EdgeRecord {
            id: GraphId::new(format!("edge:contains:{file_symbol_id}:{fence_id}"))?,
            kind: "contains".to_owned(),
            from_id: file_symbol_id.clone(),
            to_id: Some(fence_id.clone()),
            unresolved_target: None,
            provenance: provenance_for_file(
                extractor_id,
                extractor_version,
                file,
                Confidence::Exact,
                Some("containment"),
            ),
        });
        sink.push_edge(EdgeRecord {
            id: GraphId::new(format!(
                "edge:code-fence-language:{}:{}",
                file.relative_path, code_fence_index
            ))?,
            kind: "code-fence-language".to_owned(),
            from_id: fence_id,
            to_id: None,
            unresolved_target: Some(language.to_owned()),
            provenance: provenance_for_file(
                extractor_id,
                extractor_version,
                file,
                Confidence::Exact,
                Some("code-fence"),
            ),
        });
    }
    Ok(())
}

fn emit_links(
    source: &MarkdownSource<'_>,
    links: &[Link],
    emitted_link_file_refs: &mut BTreeSet<String>,
    sink: &mut GraphSink,
) -> Result<(), CodeGraphError> {
    let MarkdownSource {
        extractor_id,
        extractor_version,
        file,
        ..
    } = source;
    let file_symbol_id = source.file_symbol_id.clone();
    for link in links {
        if link.dest.contains("://") {
            continue;
        }
        let resolved_path = resolve_local_path(file, &link.dest);
        sink.push_edge(EdgeRecord {
            id: GraphId::new(format!(
                "edge:doc-link:{}:{}",
                file.relative_path, link.dest
            ))?,
            kind: "doc-link".to_owned(),
            from_id: file_symbol_id.clone(),
            to_id: None,
            unresolved_target: Some(link.dest.clone()),
            provenance: provenance_for_file(
                extractor_id,
                extractor_version,
                file,
                Confidence::Syntactic,
                Some("markdown-link"),
            ),
        });
        if let Some(path) = resolved_path {
            if emitted_link_file_refs.insert(path.clone()) {
                sink.push_edge(EdgeRecord {
                    id: GraphId::new(format!(
                        "edge:doc-link-file:{}:{}",
                        file.relative_path,
                        id_fragment(&path)
                    ))?,
                    kind: "doc-link-file".to_owned(),
                    from_id: file_symbol_id.clone(),
                    to_id: Some(file_graph_id(&path)?),
                    unresolved_target: None,
                    provenance: provenance_for_file(
                        extractor_id,
                        extractor_version,
                        file,
                        Confidence::Exact,
                        Some("markdown-link-file"),
                    ),
                });
            }
        }
    }
    Ok(())
}

fn emit_field_facts(
    source: &MarkdownSource<'_>,
    profile: &CompiledDocsProfile,
    content: &str,
    headings: &[Heading],
    fences: &[Fence],
    sink: &mut GraphSink,
) -> Result<(), CodeGraphError> {
    let MarkdownSource {
        extractor_id,
        extractor_version,
        file,
        file_record,
        ..
    } = source;
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for (start, end, line) in iter_lines(content) {
        if in_ranges(
            start,
            fences.iter().map(|fence| fence.start_byte..fence.end_byte),
        ) || in_heading_line(start, headings)
        {
            continue;
        }
        let Some((label, value)) = split_label_line(line) else {
            continue;
        };
        if value.is_empty() {
            continue;
        }
        for field in profile.fields_for_label(label) {
            let count = counts.entry(field.token.clone()).or_insert(0);
            *count += 1;
            let occurrence = *count;
            if field.single_valued && occurrence == 2 {
                sink.push_diagnostic(DiagnosticRecord {
                    id: GraphId::new(format!(
                        "diag:doc-field-duplicate:{}:{}",
                        file.relative_path, field.token
                    ))?,
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "duplicate single-valued field `{}` in `{}`",
                        field.token, file.relative_path
                    ),
                    file_id: Some(file_record.id.clone()),
                    span: Some(span_from_bytes(content, start, end)),
                    provenance: provenance_for_file(
                        extractor_id,
                        extractor_version,
                        file,
                        Confidence::Exact,
                        Some("duplicate-field"),
                    ),
                });
            }
            sink.push_symbol(SymbolRecord {
                id: GraphId::new(format!(
                    "symbol:doc-field:{}:{}:{occurrence}",
                    file.relative_path, field.token
                ))?,
                kind: "doc-field".to_owned(),
                display_name: value.to_owned(),
                canonical_name: format!("{}#{}", file.relative_path, field.token),
                file_id: file_record.id.clone(),
                span: span_from_bytes(content, start, end),
                provenance: provenance_for_file(
                    extractor_id,
                    extractor_version,
                    file,
                    Confidence::Exact,
                    Some(&field.token),
                ),
            });
        }
    }
    Ok(())
}

fn emit_typed_relations(
    source: &MarkdownSource<'_>,
    profile: &CompiledDocsProfile,
    content: &str,
    headings: &[Heading],
    fences: &[Fence],
    links: &[Link],
    sink: &mut GraphSink,
) -> Result<(), CodeGraphError> {
    let MarkdownSource {
        extractor_id,
        extractor_version,
        file,
        file_record,
        ..
    } = source;
    let file_symbol_id = source.file_symbol_id.clone();
    let mut emitted_edges = BTreeSet::new();
    for (index, link) in links.iter().enumerate() {
        if in_ranges(
            link.span.start,
            fences.iter().map(|fence| fence.start_byte..fence.end_byte),
        ) {
            continue;
        }
        let mut relation_tokens = BTreeSet::new();
        if let Some(line) = line_at(content, link.span.start) {
            if let Some((label, _)) = split_label_line(line) {
                for relation in profile.relations_for_label(label) {
                    relation_tokens.insert(relation.token.clone());
                }
            }
        }
        for heading in enclosing_headings(headings, content, link.span.start) {
            for relation in profile.relations_for_heading(&heading.text) {
                relation_tokens.insert(relation.token.clone());
            }
        }
        if relation_tokens.is_empty() || link.dest.trim().is_empty() {
            continue;
        }
        for token in relation_tokens {
            let edge_key = format!("{token}:{}", link.dest);
            if emitted_edges.insert(edge_key) {
                sink.push_edge(EdgeRecord {
                    id: GraphId::new(format!(
                        "edge:doc-rel:{}:{}:{}",
                        file.relative_path,
                        token,
                        id_fragment(&link.dest)
                    ))?,
                    kind: DOC_REL_KIND.to_owned(),
                    from_id: file_symbol_id.clone(),
                    to_id: None,
                    unresolved_target: Some(link.dest.clone()),
                    provenance: provenance_for_file(
                        extractor_id,
                        extractor_version,
                        file,
                        Confidence::Exact,
                        Some(&token),
                    ),
                });
            }
            sink.push_reference(ReferenceRecord {
                id: GraphId::new(format!(
                    "ref:doc-rel:{}:{}:{index}:{}",
                    file.relative_path,
                    token,
                    id_fragment(&link.dest)
                ))?,
                file_id: file_record.id.clone(),
                kind: DOC_REL_KIND.to_owned(),
                target_id: None,
                unresolved_target: Some(link.dest.clone()),
                span: span_from_bytes(content, link.span.start, link.span.end),
                provenance: provenance_for_file(
                    extractor_id,
                    extractor_version,
                    file,
                    Confidence::Exact,
                    Some(&token),
                ),
            });
        }
    }
    Ok(())
}

fn enclosing_headings<'a>(headings: &'a [Heading], content: &str, byte: usize) -> Vec<&'a Heading> {
    headings
        .iter()
        .enumerate()
        .filter(|(index, heading)| {
            let end = headings
                .iter()
                .skip(index + 1)
                .find(|next| next.level <= heading.level)
                .map(|next| next.start_byte)
                .unwrap_or(content.len());
            byte >= heading.start_byte && byte < end
        })
        .map(|(_, heading)| heading)
        .collect()
}

fn in_heading_line(byte: usize, headings: &[Heading]) -> bool {
    headings
        .iter()
        .any(|heading| byte >= heading.start_byte && byte < heading.heading_end_byte)
}

fn in_ranges(byte: usize, mut ranges: impl Iterator<Item = Range<usize>>) -> bool {
    ranges.any(|range| byte >= range.start && byte < range.end)
}

fn split_label_line(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim();
    if trimmed.starts_with('#') || trimmed.starts_with('|') {
        return None;
    }
    let (label, value) = trimmed.split_once(':')?;
    let label = label.trim();
    if label.is_empty() || label.chars().any(|ch| ch == '[' || ch == '`') {
        return None;
    }
    Some((label, value.trim()))
}

/// Byte range of a complete leading YAML frontmatter block, when present.
///
/// A complete block starts with a standalone `---` on the first line and ends
/// at the next standalone `---` line. The body between the fences may be empty
/// or begin with blank lines. Incomplete opening fences and later delimiter
/// shapes return `None` so ordinary Markdown heading behavior is unchanged.
fn leading_yaml_frontmatter_range(content: &str) -> Option<Range<usize>> {
    let lines = iter_lines(content);
    let (_, _, first_line) = *lines.first()?;
    if !is_yaml_frontmatter_fence(first_line) {
        return None;
    }

    for &(_start, end, line) in lines.iter().skip(1) {
        if is_yaml_frontmatter_fence(line) {
            let range_end = if end < content.len() { end + 1 } else { end };
            return Some(0..range_end);
        }
    }
    None
}

fn is_yaml_frontmatter_fence(line: &str) -> bool {
    line.trim_end() == "---"
}

fn iter_lines(content: &str) -> Vec<(usize, usize, &str)> {
    let mut lines = Vec::new();
    let mut start = 0usize;
    for (idx, ch) in content.char_indices() {
        if ch == '\n' {
            lines.push((start, idx, &content[start..idx]));
            start = idx + 1;
        }
    }
    if start <= content.len() {
        lines.push((start, content.len(), &content[start..]));
    }
    lines
}

fn line_at(content: &str, byte: usize) -> Option<&str> {
    iter_lines(content)
        .into_iter()
        .find(|(start, end, _)| byte >= *start && byte <= *end)
        .map(|(_, _, line)| line)
}
