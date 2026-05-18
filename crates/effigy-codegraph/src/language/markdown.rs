use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Parser, Tag, TagEnd};

use crate::error::CodeGraphError;
use crate::extractor::{
    capability_set, extractor_id, file_graph_id, GraphSink, LanguageIndexer, SourceFile,
};
use crate::model::{
    Confidence, EdgeRecord, ExtractorCapability, ExtractorRecord, FileRecord, SymbolRecord,
};
use crate::support::{full_span, id_fragment, normalize_rel_path, provenance_for_file};
use crate::{ExtractorId, GraphId};

pub struct MarkdownIndexer {
    extractor_id: ExtractorId,
    version: String,
}

impl MarkdownIndexer {
    pub fn new() -> Self {
        Self {
            extractor_id: extractor_id("markdown-anchors").expect("static extractor id"),
            version: "0.1.0".to_owned(),
        }
    }
}

impl LanguageIndexer for MarkdownIndexer {
    fn extractor_record(&self) -> ExtractorRecord {
        ExtractorRecord {
            id: self.extractor_id.clone(),
            version: self.version.clone(),
            language_ids: vec!["markdown".to_owned()],
            capabilities: capability_set(&[
                ExtractorCapability::Symbols,
                ExtractorCapability::Docs,
                ExtractorCapability::References,
            ]),
        }
    }

    fn supports_path(&self, relative_path: &str) -> bool {
        relative_path.ends_with(".md")
    }

    fn extract(
        &self,
        file: &SourceFile,
        file_record: &FileRecord,
        sink: &mut GraphSink,
    ) -> Result<(), CodeGraphError> {
        let parser = Parser::new(&file.content);
        let file_symbol_id = GraphId::new(format!("symbol:doc:file:{}", file.relative_path))?;
        sink.push_symbol(SymbolRecord {
            id: file_symbol_id.clone(),
            kind: document_kind(&file.relative_path).to_owned(),
            display_name: file.relative_path.clone(),
            canonical_name: file.relative_path.clone(),
            file_id: file_record.id.clone(),
            span: full_span(&file.content),
            provenance: provenance_for_file(
                &self.extractor_id,
                &self.version,
                file,
                Confidence::Exact,
                Some("document"),
            ),
        });

        let mut heading_level: Option<HeadingLevel> = None;
        let mut heading_text = String::new();
        let mut link_target: Option<String> = None;
        let mut code_fence_info: Option<String> = None;
        let mut code_fence_index = 0usize;
        let mut emitted_path_refs = BTreeSet::new();
        let mut emitted_link_file_refs = BTreeSet::new();
        for event in parser {
            match event {
                Event::Start(Tag::Heading { level, .. }) => {
                    heading_level = Some(level);
                    heading_text.clear();
                }
                Event::End(TagEnd::Heading(_)) => {
                    if let Some(level) = heading_level.take() {
                        let heading_text = heading_text.trim();
                        if heading_text.is_empty() {
                            continue;
                        }
                        let anchor = slugify(&heading_text);
                        if anchor.is_empty() {
                            continue;
                        }
                        let symbol_id =
                            GraphId::new(format!("symbol:doc:{}:#{}", file.relative_path, anchor))?;
                        sink.push_symbol(SymbolRecord {
                            id: symbol_id.clone(),
                            kind: format!("heading-h{}", heading_level_number(level)),
                            display_name: heading_text.to_owned(),
                            canonical_name: format!("{}#{}", file.relative_path, anchor),
                            file_id: file_record.id.clone(),
                            span: full_span(&file.content),
                            provenance: provenance_for_file(
                                &self.extractor_id,
                                &self.version,
                                file,
                                Confidence::Exact,
                                Some("heading"),
                            ),
                        });
                        sink.push_edge(EdgeRecord {
                            id: GraphId::new(format!(
                                "edge:contains:{file_symbol_id}:{symbol_id}"
                            ))?,
                            kind: "contains".to_owned(),
                            from_id: file_symbol_id.clone(),
                            to_id: Some(symbol_id),
                            unresolved_target: None,
                            provenance: provenance_for_file(
                                &self.extractor_id,
                                &self.version,
                                file,
                                Confidence::Exact,
                                Some("containment"),
                            ),
                        });
                    }
                }
                Event::Start(Tag::CodeBlock(kind)) => {
                    code_fence_info = match kind {
                        CodeBlockKind::Indented => Some("text".to_owned()),
                        CodeBlockKind::Fenced(info) => Some(info.to_string()),
                    };
                }
                Event::End(TagEnd::CodeBlock) => {
                    code_fence_index += 1;
                    let fence_id = GraphId::new(format!(
                        "symbol:doc:{}:fence:{}",
                        file.relative_path, code_fence_index
                    ))?;
                    let info = code_fence_info.take().unwrap_or_else(|| "text".to_owned());
                    let language = code_fence_language(&info);
                    sink.push_symbol(SymbolRecord {
                        id: fence_id.clone(),
                        kind: "code-fence".to_owned(),
                        display_name: language.to_owned(),
                        canonical_name: format!(
                            "{}::code-fence::{code_fence_index}",
                            file.relative_path
                        ),
                        file_id: file_record.id.clone(),
                        span: full_span(&file.content),
                        provenance: provenance_for_file(
                            &self.extractor_id,
                            &self.version,
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
                            &self.extractor_id,
                            &self.version,
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
                            &self.extractor_id,
                            &self.version,
                            file,
                            Confidence::Exact,
                            Some("code-fence"),
                        ),
                    });
                }
                Event::Text(text) => {
                    if heading_level.is_some() {
                        heading_text.push_str(&text);
                    }
                    for path in local_path_references(file, &text) {
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
                                    &self.extractor_id,
                                    &self.version,
                                    file,
                                    Confidence::Syntactic,
                                    Some("path-reference"),
                                ),
                            });
                        }
                    }
                }
                Event::Code(text) => {
                    for path in local_path_references(file, &text) {
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
                                    &self.extractor_id,
                                    &self.version,
                                    file,
                                    Confidence::Exact,
                                    Some("code-path-reference"),
                                ),
                            });
                        }
                    }
                }
                Event::Start(Tag::Link { dest_url, .. }) => {
                    link_target = Some(dest_url.to_string());
                }
                Event::End(TagEnd::Link) => {
                    if let Some(target) = link_target.take() {
                        if !target.contains("://") {
                            let resolved_path = resolve_local_path(file, &target);
                            sink.push_edge(EdgeRecord {
                                id: GraphId::new(format!(
                                    "edge:doc-link:{}:{}",
                                    file.relative_path, target
                                ))?,
                                kind: "doc-link".to_owned(),
                                from_id: file_symbol_id.clone(),
                                to_id: None,
                                unresolved_target: Some(target),
                                provenance: provenance_for_file(
                                    &self.extractor_id,
                                    &self.version,
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
                                            &self.extractor_id,
                                            &self.version,
                                            file,
                                            Confidence::Exact,
                                            Some("markdown-link-file"),
                                        ),
                                    });
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn code_fence_language(info: &str) -> &str {
    info.split(|ch: char| ch.is_whitespace() || ch == ',')
        .find(|part| !part.is_empty())
        .unwrap_or("text")
}

fn document_kind(path: &str) -> &'static str {
    if path.starts_with("docs/guides/") {
        "guide"
    } else if path.starts_with("docs/contracts/") {
        "contract"
    } else if path.starts_with("docs/specs/") {
        "spec"
    } else if path.starts_with("docs/roadmaps/") {
        "roadmap"
    } else if path.starts_with("docs/logs/") {
        "log"
    } else if path.ends_with("SKILL.md") {
        "skill"
    } else {
        "document"
    }
}

fn slugify(text: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in text.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    slug.trim_matches('-').to_owned()
}

fn heading_level_number(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn local_path_references(file: &SourceFile, text: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut seen = BTreeSet::new();
    for candidate in path_candidates(text) {
        if let Some(path) = resolve_local_path(file, &candidate) {
            if seen.insert(path.clone()) {
                refs.push(path);
            }
        }
    }
    refs
}

fn path_candidates(text: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    for token in text.split_whitespace() {
        let candidate = token.trim_matches(|ch: char| {
            matches!(
                ch,
                '`' | '\'' | '"' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | ',' | ';' | ':'
            )
        });
        if candidate.is_empty() || candidate.contains("://") {
            continue;
        }
        if candidate.contains('/')
            || [
                ".rs", ".md", ".toml", ".rhai", ".ts", ".tsx", ".js", ".jsx", ".php", ".json",
                ".yaml", ".yml", ".sh",
            ]
            .iter()
            .any(|ext| candidate.ends_with(ext))
        {
            candidates.push(candidate.to_owned());
        }
    }
    candidates
}

fn resolve_local_path(file: &SourceFile, target: &str) -> Option<String> {
    let path_part = target.split('#').next().unwrap_or(target).trim();
    if path_part.is_empty() {
        return None;
    }
    let absolute = if path_part.starts_with('/') {
        normalize_repo_path(file.repo_root.join(path_part.trim_start_matches('/')))
    } else {
        let base = Path::new(&file.relative_path)
            .parent()
            .unwrap_or_else(|| Path::new(""));
        normalize_repo_path(file.repo_root.join(base).join(path_part))
    };
    let relative = absolute.strip_prefix(&file.repo_root).ok()?;
    let normalized = normalize_rel_path(relative);
    let full_path = file.repo_root.join(&normalized);
    if full_path.is_file() {
        Some(normalized)
    } else {
        None
    }
}

fn normalize_repo_path(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}
