use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use effigy_codegraph::model::{Confidence, SymbolRecord};
use effigy_scan::{
    BoundaryViolationFinding, BoundaryViolationScanOptions, BoundaryViolationScanResult,
    BoundaryViolationSeverity,
};
use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::BuiltinError;

use super::graph_helpers::open_fresh_graph_store;

pub(super) fn run_boundary_violation_scan(
    target_root: &Path,
    options: &BoundaryViolationScanOptions,
) -> Result<BoundaryViolationScanResult, BuiltinError> {
    options
        .validate()
        .map_err(|error| BuiltinError::task_invocation(error.to_string()))?;

    if options.layers.is_empty() {
        return Ok(BoundaryViolationScanResult {
            root: target_root.display().to_string(),
            configured_layers: 0,
            checked_edges: 0,
            findings: Vec::new(),
        });
    }

    let store = open_fresh_graph_store(target_root, "boundary-violations")?;
    let files = store
        .list_files()
        .map_err(|error| BuiltinError::task_invocation(error.to_string()))?;
    let symbols = store
        .list_symbols()
        .map_err(|error| BuiltinError::task_invocation(error.to_string()))?;
    let edges = store
        .list_edges()
        .map_err(|error| BuiltinError::task_invocation(error.to_string()))?;

    let layer_rules = compile_layer_rules(options)?;
    let file_paths: BTreeMap<_, _> = files.into_iter().map(|file| (file.id, file.path)).collect();
    let symbols_by_id: BTreeMap<_, _> = symbols
        .into_iter()
        .map(|symbol| (symbol.id.clone(), symbol))
        .collect();
    let symbol_lookup = build_symbol_lookup(&symbols_by_id);

    let mut checked_edges = 0usize;
    let mut findings = Vec::new();
    for edge in edges {
        if edge.kind == "contains" {
            continue;
        }
        if !options.include_heuristic && edge.provenance.confidence == Confidence::Heuristic {
            continue;
        }
        let Some(source_evidence) =
            resolve_source_evidence(target_root, &edge, &symbols_by_id, &file_paths)
        else {
            continue;
        };
        let Some(target_symbol) = resolve_target_symbol(&edge, &symbols_by_id, &symbol_lookup)
        else {
            continue;
        };
        let Some(target_path) = file_paths.get(&target_symbol.file_id) else {
            continue;
        };

        let Some(source_layer) = classify_path(&source_evidence.path, &layer_rules)? else {
            continue;
        };
        let Some(target_layer) = classify_path(target_path, &layer_rules)? else {
            continue;
        };
        if source_layer == target_layer {
            continue;
        }

        checked_edges += 1;
        let source_rule = layer_rules
            .get(&source_layer)
            .expect("source layer rule should exist");
        if source_rule.may_depend_on.contains(&target_layer) {
            continue;
        }

        findings.push(BoundaryViolationFinding {
            source_layer: source_layer.clone(),
            target_layer: target_layer.clone(),
            edge_kind: edge.kind.clone(),
            source_path: source_evidence.path.clone(),
            source_line: source_evidence.line,
            source_symbol: source_evidence.symbol.clone(),
            target_path: target_path.clone(),
            target_line: target_symbol.span.start.line as usize,
            target_symbol: target_symbol.canonical_name.clone(),
            confidence: confidence_label(edge.provenance.confidence).to_owned(),
            severity: BoundaryViolationSeverity::High,
        });
    }

    findings.sort_by(|left, right| {
        left.source_layer
            .cmp(&right.source_layer)
            .then(left.target_layer.cmp(&right.target_layer))
            .then(left.source_path.cmp(&right.source_path))
            .then(left.source_line.cmp(&right.source_line))
            .then(left.edge_kind.cmp(&right.edge_kind))
    });

    Ok(BoundaryViolationScanResult {
        root: target_root.display().to_string(),
        configured_layers: layer_rules.len(),
        checked_edges,
        findings,
    })
}

struct SourceEvidence {
    path: String,
    line: usize,
    symbol: String,
}

fn resolve_source_evidence(
    target_root: &Path,
    edge: &effigy_codegraph::model::EdgeRecord,
    symbols_by_id: &BTreeMap<effigy_codegraph::GraphId, SymbolRecord>,
    file_paths: &BTreeMap<effigy_codegraph::GraphId, String>,
) -> Option<SourceEvidence> {
    if let Some(source_symbol) = symbols_by_id.get(&edge.from_id) {
        let path = file_paths.get(&source_symbol.file_id)?.clone();
        return Some(SourceEvidence {
            path,
            line: source_symbol.span.start.line as usize,
            symbol: source_symbol.canonical_name.clone(),
        });
    }
    let path = file_paths.get(&edge.from_id)?.clone();
    Some(SourceEvidence {
        line: infer_edge_line(target_root, &path, edge.unresolved_target.as_deref()),
        symbol: path.clone(),
        path,
    })
}

fn resolve_target_symbol<'a>(
    edge: &effigy_codegraph::model::EdgeRecord,
    symbols_by_id: &'a BTreeMap<effigy_codegraph::GraphId, SymbolRecord>,
    symbol_lookup: &BTreeMap<String, BTreeSet<effigy_codegraph::GraphId>>,
) -> Option<&'a SymbolRecord> {
    if let Some(target_id) = edge.to_id.as_ref() {
        return symbols_by_id.get(target_id);
    }
    let unresolved = edge.unresolved_target.as_deref()?;
    let mut candidate_ids = BTreeSet::new();
    for key in unresolved_target_keys(unresolved) {
        if let Some(ids) = symbol_lookup.get(&key) {
            candidate_ids.extend(ids.iter().cloned());
        }
    }
    if candidate_ids.len() == 1 {
        let id = candidate_ids.iter().next()?;
        return symbols_by_id.get(id);
    }
    None
}

fn build_symbol_lookup(
    symbols_by_id: &BTreeMap<effigy_codegraph::GraphId, SymbolRecord>,
) -> BTreeMap<String, BTreeSet<effigy_codegraph::GraphId>> {
    let mut lookup = BTreeMap::<String, BTreeSet<effigy_codegraph::GraphId>>::new();
    for symbol in symbols_by_id.values() {
        for key in symbol_lookup_keys(symbol) {
            lookup.entry(key).or_default().insert(symbol.id.clone());
        }
    }
    lookup
}

fn symbol_lookup_keys(symbol: &SymbolRecord) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    push_symbol_key(&mut keys, symbol.display_name.as_str());
    push_symbol_key(&mut keys, symbol.canonical_name.as_str());
    if let Some(last) = symbol.canonical_name.rsplit("::").next() {
        push_symbol_key(&mut keys, last);
    }
    keys
}

fn unresolved_target_keys(target: &str) -> BTreeSet<String> {
    let normalized = normalize_unresolved_target(target);
    let mut keys = BTreeSet::new();
    push_symbol_key(&mut keys, normalized.as_str());
    if let Some(last) = normalized.rsplit("::").next() {
        push_symbol_key(&mut keys, last);
    }
    keys
}

fn push_symbol_key(keys: &mut BTreeSet<String>, raw: &str) {
    let key = raw.trim();
    if !key.is_empty() {
        keys.insert(key.to_owned());
    }
}

fn normalize_unresolved_target(target: &str) -> String {
    target
        .trim()
        .trim_end_matches(';')
        .trim_start_matches("crate::")
        .trim_start_matches("self::")
        .trim_start_matches("super::")
        .to_owned()
}

fn infer_edge_line(target_root: &Path, relative_path: &str, needle: Option<&str>) -> usize {
    let Some(needle) = needle.map(str::trim).filter(|value| !value.is_empty()) else {
        return 1;
    };
    let Ok(contents) = std::fs::read_to_string(target_root.join(relative_path)) else {
        return 1;
    };
    contents
        .lines()
        .enumerate()
        .find_map(|(index, line)| line.contains(needle).then_some(index + 1))
        .unwrap_or(1)
}

struct CompiledLayerRule {
    matcher: GlobSet,
    may_depend_on: Vec<String>,
}

fn compile_layer_rules(
    options: &BoundaryViolationScanOptions,
) -> Result<BTreeMap<String, CompiledLayerRule>, BuiltinError> {
    let mut compiled = BTreeMap::new();
    for (name, layer) in &options.layers {
        let mut builder = GlobSetBuilder::new();
        for path in &layer.paths {
            let glob = Glob::new(path).map_err(|error| {
                BuiltinError::task_invocation(format!(
                    "invalid boundary layer glob `{path}` for layer `{name}`: {error}"
                ))
            })?;
            builder.add(glob);
        }
        let matcher = builder.build().map_err(|error| {
            BuiltinError::task_invocation(format!(
                "failed to compile boundary layer globs for `{name}`: {error}"
            ))
        })?;
        compiled.insert(
            name.clone(),
            CompiledLayerRule {
                matcher,
                may_depend_on: layer.may_depend_on.clone(),
            },
        );
    }
    Ok(compiled)
}

fn classify_path(
    path: &str,
    rules: &BTreeMap<String, CompiledLayerRule>,
) -> Result<Option<String>, BuiltinError> {
    let mut matched = Vec::new();
    for (name, rule) in rules {
        if rule.matcher.is_match(path) {
            matched.push(name.clone());
        }
    }
    match matched.len() {
        0 => Ok(None),
        1 => Ok(matched.into_iter().next()),
        _ => Err(BuiltinError::task_invocation(format!(
            "boundary layer match is ambiguous for `{path}`: {}",
            matched.join(", ")
        ))),
    }
}

fn confidence_label(confidence: Confidence) -> &'static str {
    match confidence {
        Confidence::Exact => "exact",
        Confidence::Syntactic => "syntactic",
        Confidence::Heuristic => "heuristic",
    }
}
