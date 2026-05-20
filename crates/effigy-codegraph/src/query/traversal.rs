use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::json::{
    GraphContextItemPayload, GraphExploreExcerptPayload, GraphExploreRelationPayload,
};
use crate::model::{EdgeRecord, FileRecord, SourceSpan, SymbolRecord};

use super::profile::split_identifier_token;
use super::snippets::{sectioned_snippet, ExploreExcerptSection};

#[derive(Debug, Clone)]
pub(super) struct ExploreTraversalNeighbor {
    pub(super) kind: String,
    pub(super) path: String,
    pub(super) language_id: Option<String>,
    pub(super) name: Option<String>,
    pub(super) range: Option<SourceSpan>,
    pub(super) reason: String,
    pub(super) score: usize,
}

#[derive(Debug)]
struct UnresolvedNeighborIndexes<'a> {
    symbols_by_token: BTreeMap<String, Vec<&'a SymbolRecord>>,
    files_by_token: BTreeMap<String, Vec<&'a FileRecord>>,
    files_by_id: BTreeMap<String, &'a FileRecord>,
}

pub(super) fn explore_traversal_neighbors(
    primary: &[GraphContextItemPayload],
    files: &[FileRecord],
    symbols: &[SymbolRecord],
    edges: &[EdgeRecord],
    limit: usize,
) -> Vec<ExploreTraversalNeighbor> {
    let primary_file_ids = primary
        .iter()
        .map(|item| item.record_id.as_str().to_owned())
        .collect::<BTreeSet<_>>();
    let primary_paths = primary
        .iter()
        .map(|item| item.path.as_str().to_owned())
        .collect::<BTreeSet<_>>();
    let file_by_id = files
        .iter()
        .map(|file| (file.id.as_str().to_owned(), file))
        .collect::<BTreeMap<_, _>>();
    let symbol_by_id = symbols
        .iter()
        .map(|symbol| (symbol.id.as_str().to_owned(), symbol))
        .collect::<BTreeMap<_, _>>();
    let primary_symbol_ids = symbols
        .iter()
        .filter(|symbol| primary_file_ids.contains(symbol.file_id.as_str()))
        .map(|symbol| symbol.id.as_str().to_owned())
        .collect::<BTreeSet<_>>();
    let edges_by_from = edges.iter().fold(
        BTreeMap::<String, Vec<&EdgeRecord>>::new(),
        |mut map, edge| {
            map.entry(edge.from_id.as_str().to_owned())
                .or_default()
                .push(edge);
            map
        },
    );
    let edges_by_to = edges.iter().fold(
        BTreeMap::<String, Vec<&EdgeRecord>>::new(),
        |mut map, edge| {
            if let Some(to_id) = &edge.to_id {
                map.entry(to_id.as_str().to_owned()).or_default().push(edge);
            }
            map
        },
    );
    let unresolved_indexes = build_unresolved_neighbor_indexes(files, symbols, &primary_paths);
    let mut unresolved_cache = BTreeMap::<String, Vec<ExploreTraversalNeighbor>>::new();
    let mut seen = BTreeSet::new();
    let mut neighbors = Vec::new();

    for primary_symbol_id in &primary_symbol_ids {
        if let Some(outgoing_edges) = edges_by_from.get(primary_symbol_id) {
            for edge in outgoing_edges {
                if edge.kind == "contains" {
                    continue;
                }
                if let Some(to_id) = &edge.to_id {
                    if let Some(target_symbol) = symbol_by_id.get(to_id.as_str()) {
                        push_symbol_neighbor(
                            &mut neighbors,
                            &mut seen,
                            target_symbol,
                            &primary_paths,
                            &edge.kind,
                            true,
                        );
                        if let Some(target_file) = file_by_id.get(target_symbol.file_id.as_str()) {
                            if !primary_paths.contains(target_file.path.as_str()) {
                                push_file_neighbor(
                                    &mut neighbors,
                                    &mut seen,
                                    target_file,
                                    Some(target_symbol.span.clone()),
                                    &edge.kind,
                                    true,
                                );
                            }
                        }
                    } else if let Some(target_file) = file_by_id.get(to_id.as_str()) {
                        if !primary_paths.contains(target_file.path.as_str()) {
                            push_file_neighbor(
                                &mut neighbors,
                                &mut seen,
                                target_file,
                                None,
                                &edge.kind,
                                true,
                            );
                        }
                    }
                } else if let Some(unresolved_target) = &edge.unresolved_target {
                    push_unresolved_neighbors(
                        &mut neighbors,
                        &mut seen,
                        unresolved_target,
                        &primary_paths,
                        &edge.kind,
                        true,
                        &unresolved_indexes,
                        &mut unresolved_cache,
                        4,
                    );
                } else if edge.kind == "doc-path-ref" || edge.kind == "doc-link-file" {
                    push_doc_neighbor(
                        &mut neighbors,
                        &mut seen,
                        &edge.provenance.source_path,
                        &edge.kind,
                    );
                }
            }
        }

        if let Some(incoming_edges) = edges_by_to.get(primary_symbol_id) {
            for edge in incoming_edges {
                if edge.kind == "contains" {
                    continue;
                }
                if let Some(source_symbol) = symbol_by_id.get(edge.from_id.as_str()) {
                    push_symbol_neighbor(
                        &mut neighbors,
                        &mut seen,
                        source_symbol,
                        &primary_paths,
                        &edge.kind,
                        false,
                    );
                    if let Some(source_file) = file_by_id.get(source_symbol.file_id.as_str()) {
                        if !primary_paths.contains(source_file.path.as_str()) {
                            push_file_neighbor(
                                &mut neighbors,
                                &mut seen,
                                source_file,
                                Some(source_symbol.span.clone()),
                                &edge.kind,
                                false,
                            );
                        }
                    }
                }
            }
        }
    }

    neighbors.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.name.cmp(&right.name))
    });
    neighbors.truncate(limit);
    neighbors
}

pub(super) fn append_traversal_excerpts(
    repo_root: &Path,
    traversal: &[ExploreTraversalNeighbor],
    excerpts: &mut Vec<GraphExploreExcerptPayload>,
    excerpt_paths: &mut BTreeSet<String>,
    excerpt_bytes: &mut usize,
    max_bytes: usize,
) {
    for neighbor in traversal
        .iter()
        .filter(|neighbor| neighbor.kind == "file" || neighbor.kind == "doc")
    {
        if !excerpt_paths.insert(neighbor.path.clone()) {
            continue;
        }
        let remaining_bytes = max_bytes.saturating_sub(*excerpt_bytes);
        if remaining_bytes == 0 {
            break;
        }
        let Some(section) =
            traversal_neighbor_snippet(repo_root, neighbor, remaining_bytes.min(1_200))
        else {
            continue;
        };
        *excerpt_bytes += section.text.len();
        excerpts.push(GraphExploreExcerptPayload {
            path: neighbor.path.clone(),
            language_id: neighbor.language_id.clone(),
            name: neighbor.name.clone(),
            range: neighbor.range.clone(),
            role: neighbor.kind.clone(),
            section_kind: section.section_kind,
            completeness: section.completeness,
            score: neighbor.score,
            reasons: vec![neighbor.reason.clone()],
            text: section.text,
            truncated: section.truncated,
        });
        if *excerpt_bytes >= max_bytes {
            break;
        }
    }
}

pub(super) fn excerpt_from_context_item(
    repo_root: &Path,
    item: &GraphContextItemPayload,
    remaining_bytes: usize,
) -> Option<GraphExploreExcerptPayload> {
    if remaining_bytes == 0 {
        return None;
    }
    let section = expanded_context_item_snippet(repo_root, item, remaining_bytes.min(1_200))
        .or_else(|| {
            item.snippet.clone().map(|snippet| ExploreExcerptSection {
                text: snippet,
                truncated: item.snippet_truncated,
                section_kind: "context-window".to_owned(),
                completeness: "surrounding-context".to_owned(),
            })
        })?;
    Some(GraphExploreExcerptPayload {
        path: item.path.clone(),
        language_id: item.language_id.clone(),
        name: item.name.clone(),
        range: item.range.clone(),
        role: item.kind.clone(),
        section_kind: section.section_kind,
        completeness: section.completeness,
        score: item.score,
        reasons: item.reasons.clone(),
        text: section.text,
        truncated: section.truncated,
    })
}

pub(super) fn explore_summary(
    request: &str,
    primary: &[GraphContextItemPayload],
    excerpts: &[GraphExploreExcerptPayload],
    relations: &[GraphExploreRelationPayload],
) -> String {
    let owner_count = primary.len();
    let excerpt_count = excerpts.len();
    let relation_count = relations.len();
    let top = primary
        .first()
        .map(|item| item.path.as_str())
        .unwrap_or("no primary owner");
    format!(
        "Query `{request}` selected {owner_count} primary owners, {excerpt_count} excerpts, and {relation_count} related symbols. Top owner: {top}."
    )
}

fn push_unresolved_neighbors(
    neighbors: &mut Vec<ExploreTraversalNeighbor>,
    seen: &mut BTreeSet<String>,
    unresolved_target: &str,
    primary_paths: &BTreeSet<String>,
    edge_kind: &str,
    outgoing: bool,
    indexes: &UnresolvedNeighborIndexes<'_>,
    unresolved_cache: &mut BTreeMap<String, Vec<ExploreTraversalNeighbor>>,
    limit: usize,
) {
    let cached = unresolved_cache
        .entry(unresolved_target.to_owned())
        .or_insert_with(|| unresolved_neighbor_candidates(unresolved_target, indexes, limit));
    for candidate in cached.iter().take(limit) {
        match candidate.kind.as_str() {
            "symbol" => {
                if primary_paths.contains(candidate.path.as_str()) {
                    continue;
                }
                let key = format!(
                    "symbol:{}:{}",
                    candidate.path,
                    candidate.name.as_deref().unwrap_or_default()
                );
                if !seen.insert(key) {
                    continue;
                }
            }
            "file" => {
                if primary_paths.contains(candidate.path.as_str()) {
                    continue;
                }
                let key = format!("file:{}", candidate.path);
                if !seen.insert(key) {
                    continue;
                }
            }
            _ => continue,
        }
        let mut projected = candidate.clone();
        projected.reason = traversal_reason(edge_kind, outgoing, projected.kind.as_str());
        projected.score = traversal_score(edge_kind, outgoing, projected.kind == "symbol");
        neighbors.push(projected);
    }
}

fn build_unresolved_neighbor_indexes<'a>(
    files: &'a [FileRecord],
    symbols: &'a [SymbolRecord],
    primary_paths: &BTreeSet<String>,
) -> UnresolvedNeighborIndexes<'a> {
    let mut symbols_by_token = BTreeMap::<String, Vec<&'a SymbolRecord>>::new();
    for symbol in symbols
        .iter()
        .filter(|symbol| !primary_paths.contains(symbol.provenance.source_path.as_str()))
    {
        for token in unresolved_index_tokens(&[
            symbol.display_name.as_str(),
            symbol.canonical_name.as_str(),
            symbol.provenance.source_path.as_str(),
        ]) {
            symbols_by_token.entry(token).or_default().push(symbol);
        }
    }

    let mut files_by_token = BTreeMap::<String, Vec<&'a FileRecord>>::new();
    let mut files_by_id = BTreeMap::<String, &'a FileRecord>::new();
    for file in files
        .iter()
        .filter(|file| !primary_paths.contains(file.path.as_str()))
    {
        files_by_id.insert(file.id.as_str().to_owned(), file);
        for token in unresolved_index_tokens(&[file.path.as_str()]) {
            files_by_token.entry(token).or_default().push(file);
        }
    }

    UnresolvedNeighborIndexes {
        symbols_by_token,
        files_by_token,
        files_by_id,
    }
}

fn unresolved_neighbor_candidates(
    unresolved_target: &str,
    indexes: &UnresolvedNeighborIndexes<'_>,
    limit: usize,
) -> Vec<ExploreTraversalNeighbor> {
    let target_tokens = unresolved_target_match_tokens(unresolved_target);
    if target_tokens.is_empty() {
        return Vec::new();
    }

    let mut symbol_candidates = BTreeMap::<String, (usize, &SymbolRecord)>::new();
    for token in &target_tokens {
        for symbol in indexes
            .symbols_by_token
            .get(token)
            .into_iter()
            .flatten()
            .copied()
        {
            let Some(score) = unresolved_match_score(symbol, &target_tokens) else {
                continue;
            };
            let key = symbol.id.as_str().to_owned();
            if symbol_candidates
                .get(&key)
                .is_none_or(|(current, _)| score > *current)
            {
                symbol_candidates.insert(key, (score, symbol));
            }
        }
    }

    let mut file_candidates = BTreeMap::<String, &FileRecord>::new();
    let mut candidates = symbol_candidates
        .into_values()
        .map(|(score, symbol)| {
            if let Some(file) = indexes.files_by_id.get(symbol.file_id.as_str()) {
                file_candidates.entry(file.path.clone()).or_insert(*file);
            }
            ExploreTraversalNeighbor {
                kind: "symbol".to_owned(),
                path: symbol.provenance.source_path.clone(),
                language_id: None,
                name: Some(symbol.canonical_name.clone()),
                range: Some(symbol.span.clone()),
                reason: String::new(),
                score,
            }
        })
        .collect::<Vec<_>>();

    let unresolved_target_lower = unresolved_target.to_ascii_lowercase();
    for token in &target_tokens {
        for file in indexes
            .files_by_token
            .get(token)
            .into_iter()
            .flatten()
            .copied()
        {
            let lower = file.path.to_ascii_lowercase();
            if lower.contains(&unresolved_target_lower)
                || target_tokens
                    .iter()
                    .any(|candidate| lower.contains(candidate))
            {
                file_candidates.entry(file.path.clone()).or_insert(file);
            }
        }
    }

    candidates.extend(
        file_candidates
            .into_values()
            .map(|file| ExploreTraversalNeighbor {
                kind: "file".to_owned(),
                path: file.path.clone(),
                language_id: Some(file.language_id.clone()),
                name: Some(file.path.clone()),
                range: None,
                reason: String::new(),
                score: 2,
            }),
    );

    candidates.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.name.cmp(&right.name))
    });
    candidates.truncate(limit.saturating_mul(2));
    candidates
}

fn unresolved_index_tokens(values: &[&str]) -> BTreeSet<String> {
    values
        .iter()
        .flat_map(|value| split_identifier_token(value))
        .map(|token| token.to_ascii_lowercase())
        .filter(|token| token.len() >= 3)
        .collect()
}

fn unresolved_target_match_tokens(target: &str) -> BTreeSet<String> {
    let lower = target.to_ascii_lowercase();
    let mut tokens = lower
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-')
        .filter(|token| token.len() >= 3)
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    if let Some(last) = lower
        .rsplit(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-')
        .find(|segment| !segment.is_empty())
    {
        tokens.insert(last.to_owned());
    }
    tokens
}

fn unresolved_match_score(
    symbol: &SymbolRecord,
    target_tokens: &BTreeSet<String>,
) -> Option<usize> {
    let display = symbol.display_name.to_ascii_lowercase();
    let canonical = symbol.canonical_name.to_ascii_lowercase();
    let path = symbol.provenance.source_path.to_ascii_lowercase();
    let mut score = 0usize;
    for token in target_tokens {
        if display == *token {
            score += 8;
        } else if canonical.ends_with(token) {
            score += 7;
        } else if display.contains(token) {
            score += 5;
        } else if canonical.contains(token) {
            score += 4;
        } else if path.contains(token) {
            score += 2;
        }
    }
    (score > 0).then_some(score)
}

fn push_symbol_neighbor(
    neighbors: &mut Vec<ExploreTraversalNeighbor>,
    seen: &mut BTreeSet<String>,
    symbol: &SymbolRecord,
    primary_paths: &BTreeSet<String>,
    edge_kind: &str,
    outgoing: bool,
) {
    if primary_paths.contains(symbol.provenance.source_path.as_str()) {
        return;
    }
    let key = format!("symbol:{}", symbol.id);
    if !seen.insert(key) {
        return;
    }
    neighbors.push(ExploreTraversalNeighbor {
        kind: "symbol".to_owned(),
        path: symbol.provenance.source_path.clone(),
        language_id: None,
        name: Some(symbol.canonical_name.clone()),
        range: Some(symbol.span.clone()),
        reason: traversal_reason(edge_kind, outgoing, "symbol"),
        score: traversal_score(edge_kind, outgoing, true),
    });
}

fn push_file_neighbor(
    neighbors: &mut Vec<ExploreTraversalNeighbor>,
    seen: &mut BTreeSet<String>,
    file: &FileRecord,
    range: Option<SourceSpan>,
    edge_kind: &str,
    outgoing: bool,
) {
    let key = format!("file:{}", file.id);
    if !seen.insert(key) {
        return;
    }
    neighbors.push(ExploreTraversalNeighbor {
        kind: "file".to_owned(),
        path: file.path.clone(),
        language_id: Some(file.language_id.clone()),
        name: Some(file.path.clone()),
        range,
        reason: traversal_reason(edge_kind, outgoing, "file"),
        score: traversal_score(edge_kind, outgoing, false),
    });
}

fn push_doc_neighbor(
    neighbors: &mut Vec<ExploreTraversalNeighbor>,
    seen: &mut BTreeSet<String>,
    path: &str,
    edge_kind: &str,
) {
    let key = format!("doc:{path}");
    if !seen.insert(key) {
        return;
    }
    neighbors.push(ExploreTraversalNeighbor {
        kind: "doc".to_owned(),
        path: path.to_owned(),
        language_id: Some("markdown".to_owned()),
        name: Some(path.to_owned()),
        range: None,
        reason: format!("supporting doc via `{edge_kind}` from primary owner"),
        score: 4,
    });
}

fn traversal_reason(edge_kind: &str, outgoing: bool, target: &str) -> String {
    match (edge_kind, outgoing, target) {
        ("call", true, "symbol") => "callee symbol via `call` from primary owner".to_owned(),
        ("call", true, "file") => "callee file via `call` from primary owner".to_owned(),
        ("call", false, "symbol") => "caller symbol via `call` into primary owner".to_owned(),
        ("call", false, "file") => "caller file via `call` into primary owner".to_owned(),
        ("import" | "import-file" | "include-file", true, "symbol") => {
            format!("imported symbol via `{edge_kind}` from primary owner")
        }
        ("import" | "import-file" | "include-file", true, "file") => {
            format!("imported file via `{edge_kind}` from primary owner")
        }
        ("import" | "import-file" | "include-file", false, "symbol") => {
            format!("importing symbol via `{edge_kind}` into primary owner")
        }
        ("import" | "import-file" | "include-file", false, "file") => {
            format!("importing file via `{edge_kind}` into primary owner")
        }
        ("route-handler", true, "symbol") => {
            "route handler symbol via `route-handler` from primary owner".to_owned()
        }
        ("route-handler", true, "file") => {
            "route handler file via `route-handler` from primary owner".to_owned()
        }
        ("route-handler", false, "symbol") => {
            "route entrypoint symbol via `route-handler` into primary owner".to_owned()
        }
        ("route-handler", false, "file") => {
            "route entrypoint file via `route-handler` into primary owner".to_owned()
        }
        ("entrypoint-task", true, "symbol") => {
            "task entrypoint symbol via `entrypoint-task` from primary owner".to_owned()
        }
        ("entrypoint-task", true, "file") => {
            "task entrypoint file via `entrypoint-task` from primary owner".to_owned()
        }
        ("entrypoint-task", false, "symbol") => {
            "task selector symbol via `entrypoint-task` into primary owner".to_owned()
        }
        ("entrypoint-task", false, "file") => {
            "task selector file via `entrypoint-task` into primary owner".to_owned()
        }
        _ if outgoing && target == "symbol" => {
            format!("related symbol via `{edge_kind}` from primary owner")
        }
        _ if outgoing && target == "file" => {
            format!("related file via `{edge_kind}` from primary owner")
        }
        _ if !outgoing && target == "symbol" => {
            format!("related symbol via `{edge_kind}` into primary owner")
        }
        _ => format!("related file via `{edge_kind}` into primary owner"),
    }
}

fn traversal_score(edge_kind: &str, outgoing: bool, symbol: bool) -> usize {
    let base = match edge_kind {
        "call" => 10,
        "route-handler" | "entrypoint-task" => 9,
        "include-file" | "import" | "import-file" => 8,
        "doc-link-file" | "doc-path-ref" => 4,
        _ => 5,
    };
    base + usize::from(outgoing) + usize::from(symbol)
}

fn traversal_neighbor_snippet(
    repo_root: &Path,
    neighbor: &ExploreTraversalNeighbor,
    limit: usize,
) -> Option<ExploreExcerptSection> {
    if limit == 0 {
        return None;
    }
    let content = fs::read_to_string(repo_root.join(&neighbor.path)).ok()?;
    let (start, end) = neighbor
        .range
        .as_ref()
        .map(|range| (range.start.byte as usize, range.end.byte as usize))
        .unwrap_or((0, content.len().min(limit)));
    sectioned_snippet(
        &content,
        neighbor.language_id.as_deref(),
        neighbor.kind.as_str(),
        start,
        end,
        limit,
    )
}

fn expanded_context_item_snippet(
    repo_root: &Path,
    item: &GraphContextItemPayload,
    limit: usize,
) -> Option<ExploreExcerptSection> {
    if limit == 0 {
        return None;
    }
    let content = fs::read_to_string(repo_root.join(&item.path)).ok()?;
    let (start, end) = item
        .range
        .as_ref()
        .map(|range| (range.start.byte as usize, range.end.byte as usize))
        .unwrap_or((0, content.len().min(limit)));
    sectioned_snippet(
        &content,
        item.language_id.as_deref(),
        item.kind.as_str(),
        start,
        end,
        limit,
    )
}
