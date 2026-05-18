use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::error::CodeGraphError;
use crate::json::{
    GraphContextItemPayload, GraphContextOverflowPayload, GraphContextPayload, GraphFilesPayload,
    GraphFreshnessPayload, GraphImpactPayload, GraphNodePayload, GraphRelatedNodesPayload,
    GraphSearchMatchPayload, GraphSearchPayload,
};
use crate::model::{FileRecord, SourceSpan, SymbolRecord};
use crate::storage::GraphStore;

pub fn files(repo_root: &Path, limit: Option<usize>) -> Result<GraphFilesPayload, CodeGraphError> {
    let store = GraphStore::open(repo_root)?;
    let mut files = store.list_files()?;
    if let Some(limit) = limit {
        files.truncate(limit);
    }
    Ok(GraphFilesPayload {
        freshness: freshness(repo_root, &store)?,
        files,
    })
}

pub fn search(
    repo_root: &Path,
    query: &str,
    limit: Option<usize>,
) -> Result<GraphSearchPayload, CodeGraphError> {
    let store = GraphStore::open(repo_root)?;
    let limit = limit.unwrap_or(20);
    let matches = store
        .search(query, limit)?
        .into_iter()
        .map(|(record_type, record_id, rank)| {
            search_match_payload(repo_root, &store, record_type, record_id, rank)
        })
        .collect();
    Ok(GraphSearchPayload {
        query: query.to_owned(),
        freshness: freshness(repo_root, &store)?,
        matches,
    })
}

fn search_match_payload(
    repo_root: &Path,
    store: &GraphStore,
    record_type: String,
    record_id: String,
    rank: Option<f64>,
) -> GraphSearchMatchPayload {
    match record_type.as_str() {
        "file" => match store.find_file_by_id(&record_id).ok().flatten() {
            Some(file) => GraphSearchMatchPayload {
                path: Some(file.path.clone()),
                name: Some(file.path.clone()),
                snippet: file_snippet(repo_root, &file, None, 240).map(|(snippet, _)| snippet),
                rank,
                record_type,
                record_id,
            },
            None => GraphSearchMatchPayload {
                path: None,
                name: None,
                snippet: None,
                rank,
                record_type,
                record_id,
            },
        },
        "symbol" => match store.find_symbol_by_id(&record_id).ok().flatten() {
            Some(symbol) => GraphSearchMatchPayload {
                path: Some(symbol.provenance.source_path.clone()),
                name: Some(symbol.canonical_name.clone()),
                snippet: symbol_snippet(repo_root, &symbol, 240).map(|(snippet, _)| snippet),
                rank,
                record_type,
                record_id,
            },
            None => GraphSearchMatchPayload {
                path: None,
                name: None,
                snippet: None,
                rank,
                record_type,
                record_id,
            },
        },
        _ => GraphSearchMatchPayload {
            path: None,
            name: None,
            snippet: None,
            rank,
            record_type,
            record_id,
        },
    }
}

pub fn node(repo_root: &Path, id: &str) -> Result<GraphNodePayload, CodeGraphError> {
    let store = GraphStore::open(repo_root)?;
    let file = store.find_file_by_id(id)?;
    let symbol = store.find_symbol_by_id(id)?;
    let edges = store
        .list_edges()?
        .into_iter()
        .filter(|edge| {
            edge.from_id.as_str() == id || edge.to_id.as_ref().is_some_and(|to| to.as_str() == id)
        })
        .collect();
    let file_id = file
        .as_ref()
        .map(|record| record.id.as_str().to_owned())
        .or_else(|| {
            symbol
                .as_ref()
                .map(|record| record.file_id.as_str().to_owned())
        });
    let references = store
        .list_references()?
        .into_iter()
        .filter(|reference| {
            reference.file_id.as_str() == file_id.as_deref().unwrap_or_default()
                || reference
                    .target_id
                    .as_ref()
                    .is_some_and(|target| target.as_str() == id)
        })
        .collect();
    let diagnostics = store
        .list_diagnostics()?
        .into_iter()
        .filter(|diagnostic| {
            diagnostic
                .file_id
                .as_ref()
                .is_some_and(|file_id_value| Some(file_id_value.as_str()) == file_id.as_deref())
        })
        .collect();
    Ok(GraphNodePayload {
        freshness: freshness(repo_root, &store)?,
        file,
        symbol,
        edges,
        references,
        diagnostics,
    })
}

pub fn callers(
    repo_root: &Path,
    id: &str,
    limit: Option<usize>,
) -> Result<GraphRelatedNodesPayload, CodeGraphError> {
    related(repo_root, id, limit, true)
}

pub fn callees(
    repo_root: &Path,
    id: &str,
    limit: Option<usize>,
) -> Result<GraphRelatedNodesPayload, CodeGraphError> {
    related(repo_root, id, limit, false)
}

pub fn impact(
    repo_root: &Path,
    target: &str,
    limit: Option<usize>,
) -> Result<GraphImpactPayload, CodeGraphError> {
    let store = GraphStore::open(repo_root)?;
    let files = store.list_files()?;
    let symbols = store.list_symbols()?;
    let edges = store.list_edges()?;

    if let Some(file) = files.iter().find(|file| file.path == target) {
        let file_symbols = symbols
            .iter()
            .filter(|symbol| symbol.file_id == file.id)
            .cloned()
            .collect::<Vec<_>>();
        let symbol_ids = file_symbols
            .iter()
            .map(|symbol| symbol.id.as_str().to_owned())
            .collect::<BTreeSet<_>>();
        let impact_edges = edges
            .into_iter()
            .filter(|edge| {
                symbol_ids.contains(edge.from_id.as_str())
                    || edge
                        .to_id
                        .as_ref()
                        .is_some_and(|to_id| symbol_ids.contains(to_id.as_str()))
            })
            .take(limit.unwrap_or(100))
            .collect::<Vec<_>>();
        return Ok(GraphImpactPayload {
            target: target.to_owned(),
            freshness: freshness(repo_root, &store)?,
            files: vec![file.clone()],
            symbols: file_symbols,
            edges: impact_edges,
        });
    }

    let symbol = symbols.iter().find(|symbol| {
        symbol.id.as_str() == target
            || symbol.canonical_name == target
            || symbol.display_name == target
    });
    let Some(symbol) = symbol.cloned() else {
        return Ok(GraphImpactPayload {
            target: target.to_owned(),
            freshness: freshness(repo_root, &store)?,
            files: Vec::new(),
            symbols: Vec::new(),
            edges: Vec::new(),
        });
    };
    let file = files
        .iter()
        .find(|file| file.id == symbol.file_id)
        .cloned()
        .into_iter()
        .collect::<Vec<_>>();
    let edges = edges
        .into_iter()
        .filter(|edge| {
            edge.from_id == symbol.id
                || edge.to_id.as_ref().is_some_and(|to_id| *to_id == symbol.id)
                || edge
                    .unresolved_target
                    .as_ref()
                    .is_some_and(|target_name| target_name.contains(&symbol.display_name))
        })
        .take(limit.unwrap_or(100))
        .collect();
    Ok(GraphImpactPayload {
        target: target.to_owned(),
        freshness: freshness(repo_root, &store)?,
        files: file,
        symbols: vec![symbol],
        edges,
    })
}

pub fn context(
    repo_root: &Path,
    request: &str,
    max_files: Option<usize>,
    max_bytes: Option<usize>,
    languages: &[String],
    paths: &[String],
) -> Result<GraphContextPayload, CodeGraphError> {
    let store = GraphStore::open(repo_root)?;
    let files = store.list_files()?;
    let symbols = store.list_symbols()?;
    let edges = store.list_edges()?;
    let max_files = max_files.unwrap_or(8);
    let max_bytes = max_bytes.unwrap_or(4096);
    let request_profile = RequestProfile::new(request);
    let tokens = &request_profile.match_tokens;

    let filtered_files = files
        .into_iter()
        .filter(|file| {
            (languages.is_empty()
                || languages
                    .iter()
                    .any(|language| language == &file.language_id))
                && (paths.is_empty() || paths.iter().any(|prefix| file.path.starts_with(prefix)))
        })
        .collect::<Vec<_>>();
    let file_by_id = filtered_files
        .iter()
        .cloned()
        .map(|file| (file.id.clone(), file))
        .collect::<BTreeMap<_, _>>();

    let symbols_by_file = symbols.iter().cloned().fold(
        BTreeMap::<_, Vec<SymbolRecord>>::new(),
        |mut map, symbol| {
            map.entry(symbol.file_id.clone()).or_default().push(symbol);
            map
        },
    );
    let doc_links_to_path = edges
        .iter()
        .filter(|edge| matches!(edge.kind.as_str(), "doc-path-ref" | "doc-link-file"))
        .filter_map(|edge| {
            edge.to_id
                .as_ref()
                .map(|to_id| (to_id.as_str().to_owned(), edge))
        })
        .fold(
            BTreeMap::<String, Vec<_>>::new(),
            |mut map, (file_id, edge)| {
                map.entry(file_id).or_default().push(edge.clone());
                map
            },
        );

    let mut scored = filtered_files
        .iter()
        .map(|file| {
            let mut score = 0i64;
            let mut reasons = Vec::new();
            let role = FileRole::classify(&file.path, &file.language_id);
            let role_adjustment = request_profile.role_adjustment(role);
            if role_adjustment != 0 {
                score += role_adjustment;
                reasons.push(format!(
                    "role `{}` adjusted score by {role_adjustment}",
                    role.label()
                ));
            }
            for token in tokens {
                if file.path.to_ascii_lowercase().contains(token) {
                    score += 3;
                    reasons.push(format!("path matches `{token}`"));
                }
            }
            if request_profile
                .normalized_request
                .split_whitespace()
                .all(|token| file.path.to_ascii_lowercase().contains(token))
                && !request_profile.normalized_request.is_empty()
            {
                score += 4;
                reasons.push("path contains all request terms".to_owned());
            }
            let symbol_hits = symbols_by_file
                .get(&file.id)
                .into_iter()
                .flatten()
                .filter(|symbol| {
                    tokens.iter().any(|token| {
                        symbol.display_name.to_ascii_lowercase().contains(token)
                            || symbol.canonical_name.to_ascii_lowercase().contains(token)
                    })
                })
                .map(|symbol| {
                    let mut symbol_reasons = Vec::new();
                    for token in tokens {
                        if symbol.display_name.to_ascii_lowercase().contains(token)
                            || symbol.canonical_name.to_ascii_lowercase().contains(token)
                        {
                            symbol_reasons.push(format!(
                                "symbol `{}` matches `{token}`",
                                symbol.display_name
                            ));
                        }
                    }
                    (symbol.clone(), symbol_reasons)
                })
                .collect::<Vec<_>>();
            let scored_symbol_hits = symbol_hits.len().min(5);
            score += scored_symbol_hits as i64;
            if symbol_hits.len() > scored_symbol_hits {
                reasons.push(format!(
                    "symbol match score capped at {scored_symbol_hits} of {} hits",
                    symbol_hits.len()
                ));
            }
            for (_, symbol_reasons) in symbol_hits.iter().take(8) {
                reasons.extend(symbol_reasons.clone());
            }
            if let Some(doc_edges) = doc_links_to_path.get(file.id.as_str()) {
                let scored_doc_edges = doc_edges.len().min(3);
                score += scored_doc_edges as i64;
                if doc_edges.len() > scored_doc_edges {
                    reasons.push(format!(
                        "doc link score capped at {scored_doc_edges} of {} links",
                        doc_edges.len()
                    ));
                }
                for edge in doc_edges.iter().take(3) {
                    reasons.push(format!("linked from doc `{}`", edge.provenance.source_path));
                }
            }
            reasons.sort();
            reasons.dedup();
            let evidence_span = symbol_hits.first().map(|(symbol, _)| symbol.span.clone());
            (file.clone(), score, reasons, evidence_span)
        })
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| left.0.path.cmp(&right.0.path))
    });
    let matched = scored
        .into_iter()
        .filter(|(_, score, _, _)| *score > 0)
        .collect::<Vec<_>>();
    let selected_files = matched
        .iter()
        .take(max_files)
        .map(|(file, score, reasons, evidence_span)| {
            (file.clone(), *score, reasons.clone(), evidence_span.clone())
        })
        .collect::<Vec<_>>();
    let omitted_files = matched.len().saturating_sub(selected_files.len());
    let selected_file_ids = selected_files
        .iter()
        .map(|(file, _, _, _)| file.id.clone())
        .collect::<BTreeSet<_>>();
    let selected_doc_count = selected_files
        .iter()
        .filter(|(file, _, _, _)| file.language_id == "markdown")
        .count();

    let mut symbol_candidates = symbols_by_file
        .iter()
        .filter(|(file_id, _)| selected_file_ids.contains(file_id))
        .flat_map(|(_, symbols)| symbols.iter())
        .map(|symbol| {
            let mut score = 0i64;
            let mut reasons = Vec::new();
            let role_adjustment = file_by_id
                .get(&symbol.file_id)
                .map(|file| {
                    request_profile
                        .role_adjustment(FileRole::classify(&file.path, &file.language_id))
                })
                .unwrap_or(0);
            if role_adjustment != 0 {
                score += role_adjustment;
                reasons.push(format!("owner role adjusted score by {role_adjustment}"));
            }
            for token in tokens {
                if symbol.display_name.to_ascii_lowercase().contains(token)
                    || symbol.canonical_name.to_ascii_lowercase().contains(token)
                {
                    score += 4;
                    reasons.push(format!("symbol matches `{token}`"));
                }
                if symbol
                    .provenance
                    .source_path
                    .to_ascii_lowercase()
                    .contains(token)
                {
                    score += 1;
                    reasons.push(format!("source path matches `{token}`"));
                }
            }
            (symbol.clone(), score, reasons)
        })
        .filter(|(_, score, _)| *score > 0)
        .collect::<Vec<_>>();
    symbol_candidates.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| left.0.canonical_name.cmp(&right.0.canonical_name))
    });

    let mut items = Vec::new();
    let mut used_bytes = 0usize;
    let mut omitted_items = 0usize;
    let mut omitted_symbols = 0usize;

    for (index, (file, score, reasons, evidence_span)) in selected_files.iter().enumerate() {
        let snippet = file_snippet(
            repo_root,
            file,
            evidence_span.as_ref(),
            max_bytes.saturating_sub(used_bytes),
        );
        let snippet_len = snippet
            .as_ref()
            .map(|(snippet, _)| snippet.len())
            .unwrap_or(0);
        if used_bytes + snippet_len > max_bytes && !items.is_empty() {
            omitted_items += 1;
            continue;
        }
        used_bytes += snippet_len;
        items.push(GraphContextItemPayload {
            kind: if file.language_id == "markdown" {
                "doc".to_owned()
            } else {
                "file".to_owned()
            },
            record_id: file.id.as_str().to_owned(),
            path: file.path.clone(),
            language_id: Some(file.language_id.clone()),
            name: Some(file.path.clone()),
            range: evidence_span.clone(),
            rank: index + 1,
            score: (*score).max(0) as usize,
            reasons: reasons.clone(),
            provenance: None,
            snippet: snippet.as_ref().map(|(value, _)| value.clone()),
            snippet_truncated: snippet
                .as_ref()
                .map(|(_, truncated)| *truncated)
                .unwrap_or(false),
        });
    }

    let max_symbol_items = max_files * 3;
    for (index, (symbol, score, reasons)) in symbol_candidates.iter().enumerate() {
        if index >= max_symbol_items {
            omitted_symbols += symbol_candidates.len().saturating_sub(max_symbol_items);
            break;
        }
        let snippet = symbol_snippet(repo_root, symbol, max_bytes.saturating_sub(used_bytes));
        let snippet_len = snippet
            .as_ref()
            .map(|(snippet, _)| snippet.len())
            .unwrap_or(0);
        if used_bytes + snippet_len > max_bytes && !items.is_empty() {
            omitted_items += 1;
            omitted_symbols += 1;
            continue;
        }
        used_bytes += snippet_len;
        items.push(GraphContextItemPayload {
            kind: "symbol".to_owned(),
            record_id: symbol.id.as_str().to_owned(),
            path: symbol.provenance.source_path.clone(),
            language_id: file_by_id
                .get(&symbol.file_id)
                .map(|file| file.language_id.clone()),
            name: Some(symbol.canonical_name.clone()),
            range: Some(symbol.span.clone()),
            rank: items.len() + 1,
            score: (*score).max(0) as usize,
            reasons: reasons.clone(),
            provenance: Some(symbol.provenance.clone()),
            snippet: snippet.as_ref().map(|(value, _)| value.clone()),
            snippet_truncated: snippet
                .as_ref()
                .map(|(_, truncated)| *truncated)
                .unwrap_or(false),
        });
    }

    let emitted_doc_count = items.iter().filter(|item| item.kind == "doc").count();
    let omitted_docs = selected_doc_count.saturating_sub(emitted_doc_count);

    let mut notes = Vec::new();
    if !languages.is_empty() {
        notes.push(format!("language filter: {}", languages.join(",")));
    }
    if !paths.is_empty() {
        notes.push(format!("path filter: {}", paths.join(",")));
    }
    let freshness = freshness(repo_root, &store)?;
    if freshness.stale {
        notes.push("index freshness: stale results may be incomplete".to_owned());
    }
    notes.push(format!("byte budget: {used_bytes}/{max_bytes}"));
    Ok(GraphContextPayload {
        request: request.to_owned(),
        freshness,
        items,
        overflow: GraphContextOverflowPayload {
            omitted_items,
            omitted_files,
            omitted_symbols,
            omitted_docs,
            byte_budget: max_bytes,
            used_bytes,
        },
        notes,
    })
}

fn file_snippet(
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

fn symbol_snippet(
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
        snippet.truncate(limit.saturating_sub(3));
        snippet.push_str("...");
    }
    if snippet.is_empty() {
        None
    } else {
        Some((snippet, truncated))
    }
}

fn related(
    repo_root: &Path,
    id: &str,
    limit: Option<usize>,
    inbound: bool,
) -> Result<GraphRelatedNodesPayload, CodeGraphError> {
    let store = GraphStore::open(repo_root)?;
    let symbols = store.list_symbols()?;
    let edges = store.list_edges()?;
    let target_symbol = symbols
        .iter()
        .find(|symbol| symbol.id.as_str() == id)
        .cloned();
    let mut related_edges = Vec::new();
    let mut related_symbols = Vec::new();
    if let Some(target_symbol) = target_symbol {
        let mut owners = BTreeSet::new();
        for edge in edges {
            let hit = if inbound {
                edge.to_id
                    .as_ref()
                    .is_some_and(|to_id| to_id == &target_symbol.id)
                    || edge
                        .unresolved_target
                        .as_ref()
                        .is_some_and(|name| name.contains(&target_symbol.display_name))
            } else {
                edge.from_id == target_symbol.id
            };
            if !hit {
                continue;
            }
            owners.insert(edge.from_id.clone());
            related_edges.push(edge);
        }
        let symbol_map = symbols
            .into_iter()
            .map(|symbol| (symbol.id.clone(), symbol))
            .collect::<BTreeMap<_, _>>();
        for owner in owners.into_iter().take(limit.unwrap_or(50)) {
            if let Some(symbol) = symbol_map.get(&owner) {
                related_symbols.push(symbol.clone());
            }
        }
    }
    Ok(GraphRelatedNodesPayload {
        freshness: freshness(repo_root, &store)?,
        target_id: id.to_owned(),
        nodes: related_symbols,
        edges: related_edges,
    })
}

fn freshness(
    repo_root: &Path,
    store: &GraphStore,
) -> Result<GraphFreshnessPayload, CodeGraphError> {
    let stale_paths = crate::index::stale_paths_for_repo(repo_root, store)?;
    Ok(GraphFreshnessPayload {
        stale: !stale_paths.is_empty(),
        stale_paths,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileRole {
    Implementation,
    Test,
    Docs,
    Planning,
    Fixture,
    Generated,
}

impl FileRole {
    fn classify(path: &str, language_id: &str) -> Self {
        let lower = path.to_ascii_lowercase();
        if lower.contains("/target/")
            || lower.contains("/node_modules/")
            || lower.contains("/vendor/")
            || lower.contains("/.effigy/")
        {
            return Self::Generated;
        }
        if lower.contains("/fixtures/")
            || lower.contains("/fixture/")
            || lower.contains("/examples/")
            || lower.starts_with("examples/")
        {
            return Self::Fixture;
        }
        if lower.starts_with("tests/")
            || lower.contains("/tests/")
            || lower.ends_with("/tests.rs")
            || lower.ends_with("_test.rs")
            || lower.ends_with("_tests.rs")
            || lower.ends_with(".test.ts")
            || lower.ends_with(".spec.ts")
            || lower.ends_with(".test.js")
            || lower.ends_with(".spec.js")
        {
            return Self::Test;
        }
        if lower.starts_with("docs/roadmaps/")
            || lower.starts_with("docs/specs/")
            || lower.starts_with("docs/logs/")
        {
            return Self::Planning;
        }
        if language_id == "markdown" || lower.starts_with("docs/") || lower.ends_with(".md") {
            return Self::Docs;
        }
        Self::Implementation
    }

    fn label(self) -> &'static str {
        match self {
            Self::Implementation => "implementation",
            Self::Test => "test",
            Self::Docs => "docs",
            Self::Planning => "planning",
            Self::Fixture => "fixture",
            Self::Generated => "generated",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequestIntent {
    Implementation,
    Test,
    Docs,
    General,
}

#[derive(Debug, Clone)]
struct RequestProfile {
    normalized_request: String,
    match_tokens: Vec<String>,
    intent: RequestIntent,
}

impl RequestProfile {
    fn new(request: &str) -> Self {
        let raw_tokens = request
            .split_whitespace()
            .flat_map(split_identifier_token)
            .map(|token| token.to_ascii_lowercase())
            .filter(|token| !token.is_empty())
            .collect::<Vec<_>>();
        let intent = classify_request_intent(&raw_tokens);
        let match_tokens = raw_tokens
            .iter()
            .filter(|token| !is_context_stop_word(token))
            .cloned()
            .collect::<Vec<_>>();
        Self {
            normalized_request: match_tokens.join(" "),
            match_tokens,
            intent,
        }
    }

    fn role_adjustment(&self, role: FileRole) -> i64 {
        match (self.intent, role) {
            (RequestIntent::Implementation, FileRole::Implementation) => 6,
            (RequestIntent::Implementation, FileRole::Test) => -5,
            (RequestIntent::Implementation, FileRole::Docs | FileRole::Planning) => -4,
            (RequestIntent::Implementation, FileRole::Fixture) => -3,
            (RequestIntent::Implementation, FileRole::Generated) => -8,
            (RequestIntent::Test, FileRole::Test) => 6,
            (RequestIntent::Test, FileRole::Implementation) => 2,
            (RequestIntent::Test, FileRole::Docs | FileRole::Planning) => -2,
            (RequestIntent::Docs, FileRole::Docs) => 7,
            (RequestIntent::Docs, FileRole::Planning) => 4,
            (RequestIntent::Docs, FileRole::Implementation) => -2,
            (RequestIntent::Docs, FileRole::Test) => -3,
            (RequestIntent::General, FileRole::Generated) => -6,
            (RequestIntent::General, FileRole::Planning) => -1,
            _ => 0,
        }
    }
}

fn classify_request_intent(tokens: &[String]) -> RequestIntent {
    if tokens.iter().any(|token| {
        matches!(
            token.as_str(),
            "test" | "tests" | "regression" | "fixture" | "fixtures" | "coverage"
        )
    }) {
        return RequestIntent::Test;
    }
    if tokens.iter().any(|token| {
        matches!(
            token.as_str(),
            "doc"
                | "docs"
                | "guide"
                | "guides"
                | "contract"
                | "contracts"
                | "roadmap"
                | "roadmaps"
                | "skill"
                | "skills"
        )
    }) {
        return RequestIntent::Docs;
    }
    if tokens.iter().any(|token| {
        matches!(
            token.as_str(),
            "trace"
                | "implement"
                | "implementation"
                | "owner"
                | "runtime"
                | "command"
                | "flow"
                | "where"
                | "how"
                | "understand"
                | "resolve"
                | "resolution"
        )
    }) {
        return RequestIntent::Implementation;
    }
    RequestIntent::General
}

fn is_context_stop_word(token: &str) -> bool {
    matches!(
        token,
        "trace"
            | "find"
            | "where"
            | "how"
            | "understand"
            | "implementation"
            | "implement"
            | "owner"
            | "flow"
            | "the"
            | "a"
            | "an"
            | "and"
            | "or"
            | "for"
            | "to"
            | "of"
            | "in"
    )
}

fn split_identifier_token(token: &str) -> Vec<String> {
    let cleaned = token
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-')
        .replace(['_', '-'], " ");
    let mut tokens = Vec::new();
    for part in cleaned.split_whitespace() {
        let mut current = String::new();
        for ch in part.chars() {
            if ch.is_ascii_uppercase() && !current.is_empty() {
                tokens.push(current.to_ascii_lowercase());
                current.clear();
            }
            current.push(ch);
        }
        if !current.is_empty() {
            tokens.push(current.to_ascii_lowercase());
        }
    }
    tokens
}
