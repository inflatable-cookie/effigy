use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::error::CodeGraphError;
use crate::json::{
    GraphAffectedFilePayload, GraphAffectedPayload, GraphAffectedTaskPayload,
    GraphContextItemPayload, GraphContextOverflowPayload, GraphContextPayload,
    GraphExploreEditTargetPayload, GraphExploreIndexPayload, GraphExplorePayload,
    GraphExploreRelationPayload, GraphFilesPayload, GraphFreshnessPayload, GraphImpactPayload,
    GraphNodePayload, GraphRelatedNodesPayload, GraphSearchMatchPayload, GraphSearchPayload,
};
use crate::model::{EdgeRecord, FileRecord, SymbolRecord};
use crate::storage::GraphStore;

mod profile;
mod snippets;
mod traversal;

use profile::{FileRole, RequestIntent, RequestProfile};
use snippets::{
    file_snippet, indexed_source_evidence, indexed_source_evidence_span, indexed_source_matches,
    strongest_symbol_span, symbol_snippet,
};
use traversal::{
    append_traversal_excerpts, excerpt_from_context_item, explore_summary,
    explore_traversal_neighbors,
};

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
    let freshness = freshness(repo_root, &store)?;
    affected_from_graph(
        &files,
        &symbols,
        &edges,
        freshness,
        changed_paths,
        depth,
        limit,
    )
}

fn affected_from_graph(
    files: &[FileRecord],
    symbols: &[SymbolRecord],
    edges: &[EdgeRecord],
    freshness: GraphFreshnessPayload,
    changed_paths: &[String],
    depth: usize,
    limit: usize,
) -> Result<GraphAffectedPayload, CodeGraphError> {
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
        for edge in edges {
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
        freshness,
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
    let request_profile = RequestProfile::new(request, repo_root);
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
    let edit_seed_paths = explore_edit_seed_paths(&primary);
    let projected_validation = if edit_seed_paths.is_empty() {
        None
    } else {
        Some(affected_from_graph(
            &files,
            &symbols,
            &edges,
            freshness.clone(),
            &edit_seed_paths,
            2,
            max_files * 3,
        )?)
    };
    let edit_targets = project_explore_edit_targets(
        &primary,
        projected_validation
            .as_ref()
            .map(|payload| payload.affected_files.as_slice())
            .unwrap_or(&[]),
    );
    let likely_test_files = projected_validation
        .as_ref()
        .map(|payload| payload.likely_test_files.clone())
        .unwrap_or_default();
    let likely_test_tasks = projected_validation
        .as_ref()
        .map(|payload| project_explore_test_tasks(&payload.likely_test_tasks))
        .unwrap_or_default();
    let mut guidance = context_payload.notes.clone();
    if counts.files > 0 && !context_payload.freshness.stale {
        guidance.push("index freshness: ready".to_owned());
    }
    if !traversal.is_empty() {
        guidance.push(
            "relations include bounded one-hop graph traversal from primary owners".to_owned(),
        );
    }
    if !edit_targets.is_empty() {
        guidance.push(
            "edit targets separate the top owner from adjacent wiring where graph evidence supports it"
                .to_owned(),
        );
    }
    if !likely_test_files.is_empty() || !likely_test_tasks.is_empty() {
        guidance.push(
            "likely tests are bounded graph candidates, not exhaustive validation proof".to_owned(),
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
        edit_targets,
        likely_test_files,
        likely_test_tasks,
        overflow: context_payload.overflow,
        guidance,
    })
}

fn explore_edit_seed_paths(primary: &[GraphContextItemPayload]) -> Vec<String> {
    primary
        .iter()
        .filter(|item| item.kind == "file")
        .filter(|item| {
            matches!(
                FileRole::classify(
                    item.path.as_str(),
                    item.language_id.as_deref().unwrap_or("unknown")
                ),
                FileRole::Implementation | FileRole::Config
            )
        })
        .take(1)
        .map(|item| item.path.clone())
        .collect()
}

fn project_explore_edit_targets(
    primary: &[GraphContextItemPayload],
    affected_files: &[GraphAffectedFilePayload],
) -> Vec<GraphExploreEditTargetPayload> {
    let mut targets = Vec::new();
    let mut seen_paths = BTreeSet::new();

    if let Some(owner) = primary.iter().find(|item| {
        item.kind == "file"
            && matches!(
                FileRole::classify(
                    item.path.as_str(),
                    item.language_id.as_deref().unwrap_or("unknown")
                ),
                FileRole::Implementation | FileRole::Config
            )
    }) {
        let role = FileRole::classify(
            owner.path.as_str(),
            owner.language_id.as_deref().unwrap_or("unknown"),
        );
        seen_paths.insert(owner.path.clone());
        targets.push(GraphExploreEditTargetPayload {
            kind: if role == FileRole::Config {
                "config".to_owned()
            } else {
                "implementation".to_owned()
            },
            path: owner.path.clone(),
            language_id: owner.language_id.clone(),
            range: owner.range.clone(),
            confidence: "ranked".to_owned(),
            reasons: owner.reasons.clone(),
        });
    }

    if let Some(wiring) = affected_files.iter().find(|item| {
        !seen_paths.contains(item.path.as_str())
            && item
                .reasons
                .iter()
                .any(|reason| !reason.contains("`contains`"))
            && matches!(
                FileRole::classify(
                    item.path.as_str(),
                    item.language_id.as_deref().unwrap_or("unknown")
                ),
                FileRole::Implementation | FileRole::Config
            )
    }) {
        let role = FileRole::classify(
            wiring.path.as_str(),
            wiring.language_id.as_deref().unwrap_or("unknown"),
        );
        targets.push(GraphExploreEditTargetPayload {
            kind: if role == FileRole::Config {
                "config".to_owned()
            } else {
                "wiring".to_owned()
            },
            path: wiring.path.clone(),
            language_id: wiring.language_id.clone(),
            range: None,
            confidence: wiring.confidence.clone(),
            reasons: wiring.reasons.clone(),
        });
    }

    targets
}

fn project_explore_test_tasks(tasks: &[GraphAffectedTaskPayload]) -> Vec<GraphAffectedTaskPayload> {
    tasks
        .iter()
        .filter(|task| {
            let lower = task.name.to_ascii_lowercase();
            lower.contains("test")
                || lower.contains("nextest")
                || lower.contains("vitest")
                || lower.contains("jest")
                || lower.contains("pytest")
        })
        .take(6)
        .cloned()
        .collect()
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
    crate::index::freshness_payload(repo_root, store)
}
