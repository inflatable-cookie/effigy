use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use effigy_codegraph::model::{
    Confidence, ExtractorCapability, ExtractorRecord, FileIndexStatus, SymbolRecord,
};
use effigy_codegraph::{status, GraphId, GraphStore};
use effigy_scan::{
    DeadCodeConfidence, DeadCodeFinding, DeadCodeFindingKind, DeadCodeScanOptions,
    DeadCodeScanResult, DeadCodeSeverity,
};
use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::BuiltinError;

pub(super) fn run_dead_code_scan(
    target_root: &Path,
    options: &DeadCodeScanOptions,
) -> Result<DeadCodeScanResult, BuiltinError> {
    options
        .validate()
        .map_err(|error| BuiltinError::task_invocation(error.to_string()))?;

    let graph_status =
        status(target_root).map_err(|error| BuiltinError::task_invocation(error.to_string()))?;
    if !graph_status.freshness.usable {
        return Err(BuiltinError::task_invocation(format!(
            "`scan dead-code` requires a usable graph index ({})",
            graph_status.freshness.summary
        )));
    }

    let store = GraphStore::open(target_root)
        .map_err(|error| BuiltinError::task_invocation(error.to_string()))?;
    let files = store
        .list_files()
        .map_err(|error| BuiltinError::task_invocation(error.to_string()))?;
    let symbols = store
        .list_symbols()
        .map_err(|error| BuiltinError::task_invocation(error.to_string()))?;
    let edges = store
        .list_edges()
        .map_err(|error| BuiltinError::task_invocation(error.to_string()))?;
    let references = store
        .list_references()
        .map_err(|error| BuiltinError::task_invocation(error.to_string()))?;
    let extractors = store
        .list_extractors()
        .map_err(|error| BuiltinError::task_invocation(error.to_string()))?;

    let supported_languages = supported_language_map(&extractors);
    let allow_paths = compile_globs("scan.dead_code.allow_paths", &options.allow_paths)?;
    let allow_symbols = compile_globs("scan.dead_code.allow_symbols", &options.allow_symbols)?;

    let file_by_id: BTreeMap<_, _> = files
        .into_iter()
        .map(|file| (file.id.clone(), file))
        .collect();
    let symbol_by_id: BTreeMap<_, _> = symbols
        .iter()
        .cloned()
        .map(|symbol| (symbol.id.clone(), symbol))
        .collect();
    let mut file_stats: BTreeMap<GraphId, FileStats> = BTreeMap::new();
    let mut symbol_stats: BTreeMap<GraphId, SymbolStats> = BTreeMap::new();
    let mut file_symbol_ids: BTreeMap<GraphId, Vec<GraphId>> = BTreeMap::new();

    for file_id in file_by_id.keys() {
        file_stats.entry(file_id.clone()).or_default();
    }
    for symbol in &symbols {
        file_symbol_ids
            .entry(symbol.file_id.clone())
            .or_default()
            .push(symbol.id.clone());
        file_stats.entry(symbol.file_id.clone()).or_default();
        symbol_stats.entry(symbol.id.clone()).or_default();
    }

    for edge in &edges {
        if edge.kind == "contains" {
            continue;
        }
        if !options.include_heuristic && edge.provenance.confidence == Confidence::Heuristic {
            continue;
        }
        let is_entrypoint = is_entrypoint_edge_kind(&edge.kind);

        if let Some(source_symbol) = symbol_by_id.get(&edge.from_id) {
            let stats = symbol_stats.entry(source_symbol.id.clone()).or_default();
            stats.outbound_edges += 1;
            if is_entrypoint {
                stats.entrypoint_adjacent = true;
            }
            let file_stats = file_stats.entry(source_symbol.file_id.clone()).or_default();
            file_stats.outbound_edges += 1;
            if is_entrypoint {
                file_stats.entrypoint_adjacent = true;
            }
        } else if file_by_id.contains_key(&edge.from_id) {
            let stats = file_stats.entry(edge.from_id.clone()).or_default();
            stats.outbound_edges += 1;
            if is_entrypoint {
                stats.entrypoint_adjacent = true;
            }
        }

        if let Some(target_id) = edge.to_id.as_ref() {
            if let Some(target_symbol) = symbol_by_id.get(target_id) {
                let stats = symbol_stats.entry(target_symbol.id.clone()).or_default();
                stats.inbound_edges += 1;
                if is_entrypoint {
                    stats.entrypoint_adjacent = true;
                }
                let file_stats = file_stats.entry(target_symbol.file_id.clone()).or_default();
                file_stats.inbound_edges += 1;
                if is_entrypoint {
                    file_stats.entrypoint_adjacent = true;
                }
            } else if file_by_id.contains_key(target_id) {
                let stats = file_stats.entry(target_id.clone()).or_default();
                stats.inbound_edges += 1;
                if is_entrypoint {
                    stats.entrypoint_adjacent = true;
                }
            }
        }
    }

    for reference in &references {
        if !options.include_heuristic && reference.provenance.confidence == Confidence::Heuristic {
            continue;
        }
        file_stats
            .entry(reference.file_id.clone())
            .or_default()
            .outbound_references += 1;
        if let Some(target_id) = reference.target_id.as_ref() {
            if let Some(target_symbol) = symbol_by_id.get(target_id) {
                symbol_stats
                    .entry(target_symbol.id.clone())
                    .or_default()
                    .inbound_references += 1;
                file_stats
                    .entry(target_symbol.file_id.clone())
                    .or_default()
                    .inbound_references += 1;
            }
        }
    }

    let mut checked_files = 0usize;
    let mut checked_symbols = 0usize;
    let mut skipped_allowlisted_paths = 0usize;
    let mut skipped_allowlisted_symbols = 0usize;
    let mut skipped_non_implementation_files = 0usize;
    let mut skipped_unsupported_language_files = 0usize;
    let mut isolated_file_ids = BTreeSet::new();
    let mut findings = Vec::new();

    for (file_id, file) in &file_by_id {
        if file.status != FileIndexStatus::Indexed {
            continue;
        }
        let role = classify_file_role(&file.path, &file.language_id);
        if role != FileRole::Implementation {
            skipped_non_implementation_files += 1;
            continue;
        }
        if !supported_languages
            .get(&file.language_id)
            .copied()
            .unwrap_or(false)
        {
            skipped_unsupported_language_files += 1;
            continue;
        }
        if allow_paths.is_match(&file.path) {
            skipped_allowlisted_paths += 1;
            continue;
        }

        let symbol_ids = file_symbol_ids.get(file_id).cloned().unwrap_or_default();
        if symbol_ids.is_empty() {
            continue;
        }
        checked_files += 1;

        let stats = file_stats.get(file_id).cloned().unwrap_or_default();
        if stats.entrypoint_adjacent {
            continue;
        }
        if stats.inbound_edges == 0
            && stats.outbound_edges == 0
            && stats.inbound_references == 0
            && stats.outbound_references == 0
        {
            isolated_file_ids.insert(file_id.clone());
            findings.push(DeadCodeFinding {
                kind: DeadCodeFindingKind::IsolatedFile,
                path: file.path.clone(),
                line: first_symbol_line(&symbol_ids, &symbol_by_id),
                symbol: None,
                symbol_kind: None,
                language_id: file.language_id.clone(),
                confidence: DeadCodeConfidence::High,
                severity: DeadCodeSeverity::High,
                reason: format!(
                    "{} indexed symbol(s) with no inbound or outbound graph edges or references",
                    symbol_ids.len()
                ),
                inbound_edges: 0,
                outbound_edges: 0,
                inbound_references: 0,
                outbound_references: 0,
            });
        }
    }

    for symbol in &symbols {
        let Some(file) = file_by_id.get(&symbol.file_id) else {
            continue;
        };
        if isolated_file_ids.contains(&symbol.file_id) {
            continue;
        }
        let role = classify_file_role(&file.path, &file.language_id);
        if role != FileRole::Implementation {
            continue;
        }
        if !supported_languages
            .get(&file.language_id)
            .copied()
            .unwrap_or(false)
        {
            continue;
        }
        if allow_paths.is_match(&file.path) {
            continue;
        }
        if !symbol_kind_supported(&symbol.kind) {
            continue;
        }
        checked_symbols += 1;

        let stats = symbol_stats.get(&symbol.id).cloned().unwrap_or_default();
        if stats.entrypoint_adjacent {
            continue;
        }
        if allow_symbol(&allow_symbols, symbol) {
            skipped_allowlisted_symbols += 1;
            continue;
        }
        if stats.inbound_edges == 0 && stats.inbound_references == 0 {
            findings.push(DeadCodeFinding {
                kind: DeadCodeFindingKind::UnreferencedSymbol,
                path: file.path.clone(),
                line: symbol.span.start.line as usize,
                symbol: Some(symbol.canonical_name.clone()),
                symbol_kind: Some(symbol.kind.clone()),
                language_id: file.language_id.clone(),
                confidence: DeadCodeConfidence::Medium,
                severity: DeadCodeSeverity::Warning,
                reason: if stats.outbound_edges == 0 && stats.outbound_references == 0 {
                    "symbol has no inbound edges or references and no outward graph activity"
                        .to_owned()
                } else {
                    "symbol has outward graph activity but no known inbound edges or references"
                        .to_owned()
                },
                inbound_edges: stats.inbound_edges,
                outbound_edges: stats.outbound_edges,
                inbound_references: stats.inbound_references,
                outbound_references: stats.outbound_references,
            });
        }
    }

    findings.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.line.cmp(&right.line))
            .then(left.symbol.cmp(&right.symbol))
    });

    Ok(DeadCodeScanResult {
        root: target_root.display().to_string(),
        checked_files,
        checked_symbols,
        skipped_allowlisted_paths,
        skipped_allowlisted_symbols,
        skipped_non_implementation_files,
        skipped_unsupported_language_files,
        findings,
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
    Script,
    Migration,
}

fn classify_file_role(path: &str, language_id: &str) -> FileRole {
    let lower = path.to_ascii_lowercase();
    if lower.contains("/target/")
        || lower.contains("/node_modules/")
        || lower.contains("/vendor/")
        || lower.contains("/.effigy/")
    {
        return FileRole::Generated;
    }
    if lower.contains("/fixtures/")
        || lower.contains("/fixture/")
        || lower.starts_with("examples/")
        || lower.contains("/examples/")
    {
        return FileRole::Fixture;
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
        return FileRole::Test;
    }
    if lower.starts_with("docs/roadmaps/")
        || lower.starts_with("docs/specs/")
        || lower.starts_with("docs/logs/")
    {
        return FileRole::Planning;
    }
    if language_id == "markdown" || lower.starts_with("docs/") || lower.ends_with(".md") {
        return FileRole::Docs;
    }
    if lower.starts_with("migrations/")
        || lower.contains("/migrations/")
        || lower.contains("/db/migrate/")
        || lower.contains("/database/migrations/")
    {
        return FileRole::Migration;
    }
    if lower.starts_with("scripts/")
        || lower.starts_with("bin/")
        || lower.starts_with("cmd/")
        || lower.contains("/scripts/")
        || lower.contains("/src/bin/")
        || lower.ends_with("/lib.rs")
        || lower.ends_with("/main.rs")
        || lower.ends_with("/main.ts")
        || lower.ends_with("/main.js")
        || lower.ends_with("/main.py")
        || lower.ends_with("/main.php")
    {
        return FileRole::Script;
    }
    if lower.starts_with("config/")
        || lower.ends_with(".toml")
        || lower.ends_with(".json")
        || lower.ends_with(".yaml")
        || lower.ends_with(".yml")
    {
        return FileRole::Config;
    }
    FileRole::Implementation
}

#[derive(Debug, Clone, Default)]
struct FileStats {
    inbound_edges: usize,
    outbound_edges: usize,
    inbound_references: usize,
    outbound_references: usize,
    entrypoint_adjacent: bool,
}

#[derive(Debug, Clone, Default)]
struct SymbolStats {
    inbound_edges: usize,
    outbound_edges: usize,
    inbound_references: usize,
    outbound_references: usize,
    entrypoint_adjacent: bool,
}

fn compile_globs(label: &str, patterns: &[String]) -> Result<GlobSet, BuiltinError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|error| {
            BuiltinError::task_invocation(format!("invalid `{label}` glob `{pattern}`: {error}"))
        })?;
        builder.add(glob);
    }
    builder.build().map_err(|error| {
        BuiltinError::task_invocation(format!("failed to compile `{label}` patterns: {error}"))
    })
}

fn supported_language_map(extractors: &[ExtractorRecord]) -> BTreeMap<String, bool> {
    let mut map = BTreeMap::new();
    for extractor in extractors {
        let has_symbols = extractor
            .capabilities
            .contains(&ExtractorCapability::Symbols);
        let has_relations = extractor
            .capabilities
            .contains(&ExtractorCapability::References)
            || extractor.capabilities.contains(&ExtractorCapability::Calls)
            || extractor
                .capabilities
                .contains(&ExtractorCapability::Imports);
        let supported = has_symbols && has_relations;
        for language in &extractor.language_ids {
            map.entry(language.clone())
                .and_modify(|value| *value |= supported)
                .or_insert(supported);
        }
    }
    map
}

fn allow_symbol(patterns: &GlobSet, symbol: &SymbolRecord) -> bool {
    patterns.is_match(&symbol.canonical_name) || patterns.is_match(&symbol.display_name)
}

fn symbol_kind_supported(kind: &str) -> bool {
    !matches!(
        kind.to_ascii_lowercase().as_str(),
        "file" | "module" | "namespace" | "directory" | "package" | "root"
    )
}

fn is_entrypoint_edge_kind(kind: &str) -> bool {
    kind == "route-handler" || kind.starts_with("entrypoint-")
}

fn first_symbol_line(
    symbol_ids: &[GraphId],
    symbol_by_id: &BTreeMap<GraphId, SymbolRecord>,
) -> usize {
    symbol_ids
        .iter()
        .filter_map(|id| symbol_by_id.get(id))
        .map(|symbol| symbol.span.start.line as usize)
        .min()
        .unwrap_or(1)
}
