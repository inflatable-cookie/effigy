use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::error::CodeGraphError;
use crate::json::{
    GraphAffectedFilePayload, GraphAffectedPayload, GraphAffectedTaskPayload,
    GraphContextItemPayload, GraphContextOverflowPayload, GraphContextPayload,
    GraphExploreExcerptPayload, GraphExploreIndexPayload, GraphExplorePayload,
    GraphExploreRelationPayload, GraphFilesPayload, GraphFreshnessPayload, GraphImpactPayload,
    GraphNodePayload, GraphRelatedNodesPayload, GraphSearchMatchPayload, GraphSearchPayload,
};
use crate::model::{EdgeRecord, FileRecord, SourceSpan, SymbolRecord};
use crate::storage::GraphStore;
use crate::support::span_from_bytes;

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

pub fn affected(
    repo_root: &Path,
    changed_paths: &[String],
    depth: usize,
    limit: Option<usize>,
) -> Result<GraphAffectedPayload, CodeGraphError> {
    let store = GraphStore::open(repo_root)?;
    let files = store.list_files()?;
    let symbols = store.list_symbols()?;
    let edges = store.list_edges()?;
    let limit = limit.unwrap_or(100);
    let depth = depth.max(1);

    let file_by_path = files
        .iter()
        .map(|file| (file.path.as_str().to_owned(), file))
        .collect::<BTreeMap<_, _>>();
    let file_by_id = files
        .iter()
        .map(|file| (file.id.as_str().to_owned(), file))
        .collect::<BTreeMap<_, _>>();
    let symbols_by_file = symbols.iter().fold(
        BTreeMap::<String, Vec<&SymbolRecord>>::new(),
        |mut map, symbol| {
            map.entry(symbol.file_id.as_str().to_owned())
                .or_default()
                .push(symbol);
            map
        },
    );
    let symbol_by_id = symbols
        .iter()
        .map(|symbol| (symbol.id.as_str().to_owned(), symbol))
        .collect::<BTreeMap<_, _>>();

    let changed_paths = changed_paths
        .iter()
        .map(|path| path.trim())
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let mut queue = std::collections::VecDeque::new();
    let mut visited = BTreeSet::new();
    let mut file_reasons = BTreeMap::<String, BTreeSet<String>>::new();

    for path in &changed_paths {
        file_reasons
            .entry(path.clone())
            .or_default()
            .insert("changed input path".to_owned());
        if let Some(file) = file_by_path.get(path) {
            queue.push_back((file.id.as_str().to_owned(), 0usize));
            visited.insert(file.id.as_str().to_owned());
            if let Some(file_symbols) = symbols_by_file.get(file.id.as_str()) {
                for symbol in file_symbols {
                    let symbol_id = symbol.id.as_str().to_owned();
                    if visited.insert(symbol_id.clone()) {
                        queue.push_back((symbol_id, 0usize));
                    }
                }
            }
        }
    }

    let changed_symbol_names = changed_paths
        .iter()
        .filter_map(|path| file_by_path.get(path))
        .flat_map(|file| {
            symbols_by_file
                .get(file.id.as_str())
                .into_iter()
                .flat_map(|items| items.iter())
        })
        .map(|symbol| symbol.display_name.clone())
        .collect::<BTreeSet<_>>();

    while let Some((node_id, current_depth)) = queue.pop_front() {
        if current_depth >= depth {
            continue;
        }
        for edge in &edges {
            if edge.from_id.as_str() == node_id {
                if let Some(to_id) = &edge.to_id {
                    let next_id = to_id.as_str().to_owned();
                    if visited.insert(next_id.clone()) {
                        queue.push_back((next_id.clone(), current_depth + 1));
                        record_affected_reason(
                            &next_id,
                            &file_by_id,
                            &symbol_by_id,
                            &mut file_reasons,
                            format!("reachable via `{}` from changed node", edge.kind),
                        );
                    }
                }
            } else if edge
                .to_id
                .as_ref()
                .is_some_and(|to_id| to_id.as_str() == node_id)
            {
                let next_id = edge.from_id.as_str().to_owned();
                if visited.insert(next_id.clone()) {
                    queue.push_back((next_id.clone(), current_depth + 1));
                    record_affected_reason(
                        &next_id,
                        &file_by_id,
                        &symbol_by_id,
                        &mut file_reasons,
                        format!("incoming `{}` reaches changed node", edge.kind),
                    );
                }
            } else if let Some(target) = &edge.unresolved_target {
                if changed_symbol_names
                    .iter()
                    .any(|name| target.contains(name))
                    && visited.insert(edge.from_id.as_str().to_owned())
                {
                    let next_id = edge.from_id.as_str().to_owned();
                    queue.push_back((next_id.clone(), current_depth + 1));
                    record_affected_reason(
                        &next_id,
                        &file_by_id,
                        &symbol_by_id,
                        &mut file_reasons,
                        format!("unresolved `{}` references changed symbol", edge.kind),
                    );
                }
            }
        }
    }

    let mut affected_files = file_reasons
        .into_iter()
        .map(|(path, reasons)| {
            let file = file_by_path.get(path.as_str());
            GraphAffectedFilePayload {
                path: path.clone(),
                language_id: file.map(|record| record.language_id.clone()),
                confidence: if changed_paths.contains(&path) {
                    "exact".to_owned()
                } else if reasons.iter().any(|reason| reason.contains("unresolved")) {
                    "heuristic".to_owned()
                } else {
                    "exact".to_owned()
                },
                reasons: reasons.into_iter().collect::<Vec<_>>(),
            }
        })
        .collect::<Vec<_>>();
    affected_files.sort_by(|left, right| left.path.cmp(&right.path));
    affected_files.truncate(limit);

    let likely_test_files = affected_files
        .iter()
        .filter(|item| {
            matches!(
                FileRole::classify(
                    item.path.as_str(),
                    item.language_id.as_deref().unwrap_or("unknown")
                ),
                FileRole::Test
            )
        })
        .cloned()
        .collect::<Vec<_>>();

    let likely_test_tasks = symbols
        .iter()
        .filter(|symbol| {
            matches!(symbol.kind.as_str(), "task" | "test-suite" | "test-runner")
                && likely_test_task_name(symbol)
        })
        .take(limit)
        .map(|symbol| GraphAffectedTaskPayload {
            name: symbol.display_name.clone(),
            kind: symbol.kind.clone(),
            path: symbol.provenance.source_path.clone(),
            confidence: if symbol.kind == "task" {
                "heuristic".to_owned()
            } else {
                "exact".to_owned()
            },
            reasons: vec![
                "manifest test workflow symbol matches affected-test query surface".to_owned(),
            ],
        })
        .collect::<Vec<_>>();

    let mut notes = vec![
        "affected output is bounded graph evidence, not exhaustive test proof".to_owned(),
        "use exact confidence for resolved graph links and heuristic confidence for unresolved symbol matching".to_owned(),
    ];
    if likely_test_files.is_empty() && likely_test_tasks.is_empty() {
        notes.push(
            "no likely test files or tasks were discovered from the current graph slice".to_owned(),
        );
    }

    Ok(GraphAffectedPayload {
        changed_paths,
        freshness: freshness(repo_root, &store)?,
        depth,
        affected_files,
        likely_test_files,
        likely_test_tasks,
        notes,
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
    let freshness = freshness(repo_root, &store)?;
    context_from_graph(
        repo_root, &store, request, max_files, max_bytes, languages, paths, &files, &symbols,
        &edges, freshness,
    )
}

fn context_from_graph(
    repo_root: &Path,
    store: &GraphStore,
    request: &str,
    max_files: Option<usize>,
    max_bytes: Option<usize>,
    languages: &[String],
    paths: &[String],
    files: &[FileRecord],
    symbols: &[SymbolRecord],
    edges: &[EdgeRecord],
    freshness: GraphFreshnessPayload,
) -> Result<GraphContextPayload, CodeGraphError> {
    let max_files = max_files.unwrap_or(8);
    let max_bytes = max_bytes.unwrap_or(4096);
    let request_profile = RequestProfile::new(request);
    let tokens = &request_profile.match_tokens;

    let filtered_files = files
        .iter()
        .filter(|file| {
            (languages.is_empty()
                || languages
                    .iter()
                    .any(|language| language == &file.language_id))
                && (paths.is_empty() || paths.iter().any(|prefix| file.path.starts_with(prefix)))
        })
        .cloned()
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
    let indexed_source_matches = indexed_source_matches(
        store,
        tokens,
        filtered_files.iter().map(|file| file.id.as_str()),
    )?;

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
            if request_profile.prefers_crate_root()
                && file.path.ends_with("/src/lib.rs")
                && role == FileRole::Implementation
            {
                score += 2;
                reasons.push("crate root boosted for architecture-style request".to_owned());
            }
            let mut evidence_tokens = BTreeSet::new();
            for token in tokens {
                if file.path.to_ascii_lowercase().contains(token) {
                    score += 3;
                    evidence_tokens.insert(token.clone());
                    reasons.push(format!("path matches `{token}`"));
                }
            }
            if request_profile
                .normalized_request
                .split_whitespace()
                .all(|token| file.path.to_ascii_lowercase().contains(token))
                && !request_profile.normalized_request.is_empty()
                && !matches!(
                    (request_profile.intent, role),
                    (
                        RequestIntent::Implementation,
                        FileRole::Docs | FileRole::Planning | FileRole::Config
                    )
                )
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
                    let mut symbol_tokens = BTreeSet::new();
                    for token in tokens {
                        if symbol.display_name.to_ascii_lowercase().contains(token)
                            || symbol.canonical_name.to_ascii_lowercase().contains(token)
                        {
                            symbol_tokens.insert(token.clone());
                            symbol_reasons.push(format!(
                                "symbol `{}` matches `{token}`",
                                symbol.display_name
                            ));
                        }
                    }
                    (symbol.clone(), symbol_tokens, symbol_reasons)
                })
                .collect::<Vec<_>>();
            let symbol_tokens = symbol_hits
                .iter()
                .flat_map(|(_, symbol_tokens, _)| symbol_tokens.iter().cloned())
                .collect::<BTreeSet<_>>();
            evidence_tokens.extend(symbol_tokens.iter().cloned());
            let scored_symbol_tokens = symbol_tokens.len().min(4);
            score += (scored_symbol_tokens as i64) * 2;
            if symbol_tokens.len() > scored_symbol_tokens {
                reasons.push(format!(
                    "symbol token score capped at {scored_symbol_tokens} of {} tokens",
                    symbol_tokens.len()
                ));
            }
            for (_, _, symbol_reasons) in symbol_hits.iter().take(8) {
                reasons.extend(symbol_reasons.clone());
            }
            let source_evidence = indexed_source_evidence(
                indexed_source_matches.get(file.id.as_str()),
                role,
                request_profile.intent,
            );
            evidence_tokens.extend(source_evidence.tokens.iter().cloned());
            score += source_evidence.score;
            reasons.extend(source_evidence.reasons.clone());
            if let Some(doc_edges) = doc_links_to_path.get(file.id.as_str()) {
                let scored_doc_edges = doc_edges.len().min(3);
                let doc_edge_score = if matches!(
                    (request_profile.intent, role),
                    (
                        RequestIntent::Implementation,
                        FileRole::Docs | FileRole::Planning | FileRole::Config
                    )
                ) {
                    0
                } else {
                    scored_doc_edges as i64
                };
                score += doc_edge_score;
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
            if !evidence_tokens.is_empty() {
                let scored_evidence_tokens = evidence_tokens.len().min(5);
                score += (scored_evidence_tokens as i64) * 2;
                reasons.push(format!(
                    "covers {scored_evidence_tokens} distinct request token(s)"
                ));
                if evidence_tokens.len() >= 3 {
                    score += 4;
                    reasons.push("covers broad request evidence".to_owned());
                }
            }
            reasons.sort();
            reasons.dedup();
            let evidence_span = strongest_symbol_span(&symbol_hits);
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
            let role = FileRole::classify(&file.path, &file.language_id);
            let source_tokens = indexed_source_matches
                .get(file.id.as_str())
                .cloned()
                .unwrap_or_default();
            let resolved_span = evidence_span.clone().or_else(|| {
                indexed_source_evidence_span(
                    repo_root,
                    file,
                    &source_tokens,
                    role,
                    request_profile.intent,
                )
            });
            (file.clone(), *score, reasons.clone(), resolved_span)
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

pub fn explore(
    repo_root: &Path,
    request: &str,
    max_files: Option<usize>,
    max_bytes: Option<usize>,
    languages: &[String],
    paths: &[String],
) -> Result<GraphExplorePayload, CodeGraphError> {
    let max_files = max_files.unwrap_or(6);
    let max_bytes = max_bytes.unwrap_or(12_288);
    let store = GraphStore::open(repo_root)?;
    let files = store.list_files()?;
    let symbols = store.list_symbols()?;
    let edges = store.list_edges()?;
    let freshness = freshness(repo_root, &store)?;
    let context_payload = context_from_graph(
        repo_root,
        &store,
        request,
        Some(max_files),
        Some(max_bytes),
        languages,
        paths,
        &files,
        &symbols,
        &edges,
        freshness.clone(),
    )?;
    let counts = store.counts()?;
    let primary = context_payload
        .items
        .iter()
        .filter(|item| item.kind == "file" || item.kind == "doc")
        .take(max_files)
        .cloned()
        .collect::<Vec<_>>();
    let traversal = explore_traversal_neighbors(&primary, &files, &symbols, &edges, max_files * 4);
    let mut excerpt_bytes = 0usize;
    let mut excerpts = Vec::new();
    let mut excerpt_paths = BTreeSet::new();
    for item in &context_payload.items {
        if !excerpt_paths.insert(item.path.clone()) {
            continue;
        }
        let remaining_bytes = max_bytes.saturating_sub(excerpt_bytes);
        let Some(excerpt) = excerpt_from_context_item(repo_root, item, remaining_bytes) else {
            continue;
        };
        excerpt_bytes += excerpt.text.len();
        excerpts.push(excerpt);
        if excerpt_bytes >= max_bytes {
            break;
        }
    }
    append_traversal_excerpts(
        repo_root,
        &traversal,
        &mut excerpts,
        &mut excerpt_paths,
        &mut excerpt_bytes,
        max_bytes,
    );
    let relations = traversal
        .iter()
        .map(|neighbor| GraphExploreRelationPayload {
            kind: neighbor.kind.clone(),
            path: neighbor.path.clone(),
            name: neighbor.name.clone(),
            range: neighbor.range.clone(),
            reason: neighbor.reason.clone(),
        })
        .chain(
            context_payload
                .items
                .iter()
                .filter(|item| item.kind == "symbol")
                .take(max_files * 3)
                .map(|item| GraphExploreRelationPayload {
                    kind: "symbol".to_owned(),
                    path: item.path.clone(),
                    name: item.name.clone(),
                    range: item.range.clone(),
                    reason: item
                        .reasons
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "selected from ranked graph context".to_owned()),
                }),
        )
        .collect::<Vec<_>>();
    let mut guidance = context_payload.notes.clone();
    if counts.files > 0 && !context_payload.freshness.stale {
        guidance.push("index freshness: ready".to_owned());
    }
    if !traversal.is_empty() {
        guidance.push(
            "relations include bounded one-hop graph traversal from primary owners".to_owned(),
        );
    }
    if excerpts
        .iter()
        .any(|excerpt| excerpt.completeness != "complete-section")
    {
        guidance.push(
            "check excerpt completeness before skipping file opens; incomplete sections are labeled explicitly"
                .to_owned(),
        );
    }
    guidance.push("use returned excerpts for first-pass orientation".to_owned());
    guidance.push("use `rg` for exact token verification or missing symbols".to_owned());
    guidance
        .push("open returned files only when excerpts are insufficient for the edit".to_owned());

    Ok(GraphExplorePayload {
        query: request.to_owned(),
        index: GraphExploreIndexPayload { freshness, counts },
        summary: explore_summary(request, &primary, &excerpts, &relations),
        primary,
        excerpts,
        relations,
        overflow: context_payload.overflow,
        guidance,
    })
}

#[derive(Debug, Clone)]
struct ExploreTraversalNeighbor {
    kind: String,
    path: String,
    language_id: Option<String>,
    name: Option<String>,
    range: Option<SourceSpan>,
    reason: String,
    score: usize,
}

#[derive(Debug, Clone)]
struct ExploreExcerptSection {
    text: String,
    truncated: bool,
    section_kind: String,
    completeness: String,
}

#[derive(Debug)]
struct UnresolvedNeighborIndexes<'a> {
    symbols_by_token: BTreeMap<String, Vec<&'a SymbolRecord>>,
    files_by_token: BTreeMap<String, Vec<&'a FileRecord>>,
    files_by_id: BTreeMap<String, &'a FileRecord>,
}

fn explore_traversal_neighbors(
    primary: &[GraphContextItemPayload],
    files: &[FileRecord],
    symbols: &[SymbolRecord],
    edges: &[crate::model::EdgeRecord],
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

fn append_traversal_excerpts(
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

fn excerpt_from_context_item(
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

fn explore_summary(
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

#[derive(Debug, Clone)]
struct SourceEvidence {
    tokens: BTreeSet<String>,
    score: i64,
    reasons: Vec<String>,
}

fn indexed_source_matches<'a>(
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

fn indexed_source_evidence(
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

fn indexed_source_evidence_span(
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

fn strongest_symbol_span(
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

fn sectioned_snippet(
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

fn record_affected_reason(
    node_id: &str,
    file_by_id: &BTreeMap<String, &FileRecord>,
    symbol_by_id: &BTreeMap<String, &SymbolRecord>,
    file_reasons: &mut BTreeMap<String, BTreeSet<String>>,
    reason: String,
) {
    if let Some(file) = file_by_id.get(node_id) {
        file_reasons
            .entry(file.path.clone())
            .or_default()
            .insert(reason);
    } else if let Some(symbol) = symbol_by_id.get(node_id) {
        file_reasons
            .entry(symbol.provenance.source_path.clone())
            .or_default()
            .insert(reason);
    }
}

fn likely_test_task_name(symbol: &SymbolRecord) -> bool {
    let lower = format!(
        "{} {}",
        symbol.display_name.to_ascii_lowercase(),
        symbol.canonical_name.to_ascii_lowercase()
    );
    lower.contains("test")
        || lower.contains("qa")
        || lower.contains("nextest")
        || lower.contains("vitest")
        || lower.contains("jest")
        || lower.contains("pytest")
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
    Config,
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
        if lower.starts_with("config/")
            || lower.ends_with(".toml")
            || lower.ends_with(".json")
            || lower.ends_with(".yaml")
            || lower.ends_with(".yml")
        {
            return Self::Config;
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
            Self::Config => "config",
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
            .flat_map(|token| expanded_match_tokens(token))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        Self {
            normalized_request: match_tokens.join(" "),
            match_tokens,
            intent,
        }
    }

    fn prefers_crate_root(&self) -> bool {
        self.match_tokens
            .iter()
            .any(|token| matches!(token.as_str(), "orchestration" | "architecture"))
    }

    fn role_adjustment(&self, role: FileRole) -> i64 {
        match (self.intent, role) {
            (RequestIntent::Implementation, FileRole::Implementation) => 6,
            (RequestIntent::Implementation, FileRole::Config) => -2,
            (RequestIntent::Implementation, FileRole::Test) => -5,
            (RequestIntent::Implementation, FileRole::Docs | FileRole::Planning) => -8,
            (RequestIntent::Implementation, FileRole::Fixture) => -3,
            (RequestIntent::Implementation, FileRole::Generated) => -8,
            (RequestIntent::Test, FileRole::Test) => 6,
            (RequestIntent::Test, FileRole::Implementation) => 2,
            (RequestIntent::Test, FileRole::Docs | FileRole::Planning) => -2,
            (RequestIntent::Docs, FileRole::Docs) => 7,
            (RequestIntent::Docs, FileRole::Planning) => 4,
            (RequestIntent::Docs, FileRole::Implementation) => -2,
            (RequestIntent::Docs, FileRole::Config) => -1,
            (RequestIntent::Docs, FileRole::Test) => -3,
            (RequestIntent::General, FileRole::Generated) => -6,
            (RequestIntent::General, FileRole::Docs | FileRole::Planning) => -5,
            (RequestIntent::General, FileRole::Config) => -2,
            (RequestIntent::General, FileRole::Implementation) => 3,
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
                | "find"
                | "where"
                | "how"
                | "change"
                | "changes"
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
            | "what"
            | "when"
            | "why"
            | "understand"
            | "implementation"
            | "implement"
            | "owner"
            | "flow"
            | "does"
            | "do"
            | "did"
            | "the"
            | "this"
            | "that"
            | "a"
            | "an"
            | "and"
            | "or"
            | "for"
            | "to"
            | "of"
            | "in"
            | "with"
            | "by"
            | "from"
            | "on"
            | "at"
            | "as"
            | "if"
            | "then"
            | "is"
            | "are"
    )
}

fn expanded_match_tokens(token: &str) -> Vec<String> {
    let mut tokens = vec![token.to_owned()];
    match token {
        "change" | "changes" | "changed" => {
            tokens.extend(
                ["change", "changes", "changed"]
                    .into_iter()
                    .map(str::to_owned),
            );
        }
        "detect" | "detection" | "detected" => {
            tokens.extend(
                ["detect", "detection", "scan"]
                    .into_iter()
                    .map(str::to_owned),
            );
        }
        "stale" | "staleness" => {
            tokens.extend(
                ["stale", "staleness", "freshness"]
                    .into_iter()
                    .map(str::to_owned),
            );
        }
        "route" | "routes" | "routing" | "routed" => {
            tokens.extend(
                ["route", "routes", "routing", "selector", "selectors"]
                    .into_iter()
                    .map(str::to_owned),
            );
        }
        "parse" | "parsed" | "parser" | "parsing" => {
            tokens.extend(
                ["parse", "parsed", "parsing"]
                    .into_iter()
                    .map(str::to_owned),
            );
        }
        _ => {}
    }
    tokens
}

fn split_identifier_token(token: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let lowered = token.to_ascii_lowercase();
    let route_token = lowered
        .trim_matches(|ch: char| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    '"' | '\'' | ',' | '.' | '?' | '!' | '(' | ')' | '[' | ']'
                )
        })
        .to_owned();
    if route_token.contains('/') && route_token.chars().any(|ch| ch.is_ascii_alphanumeric()) {
        tokens.push(route_token.clone());
        for segment in route_token.split('/') {
            let cleaned = segment.trim_matches(|ch: char| !ch.is_ascii_alphanumeric());
            if !cleaned.is_empty() {
                tokens.push(cleaned.to_owned());
            }
        }
    }
    let cleaned = token
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-')
        .replace(['_', '-'], " ");
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
    tokens.sort();
    tokens.dedup();
    tokens
}
