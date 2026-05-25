use std::collections::BTreeMap;
use std::path::Path;

use effigy_codegraph::model::{Confidence, SymbolRecord};
use effigy_codegraph::{status, GraphStore};
use effigy_scan::{
    BoundaryViolationFinding, BoundaryViolationScanOptions, BoundaryViolationScanResult,
    BoundaryViolationSeverity,
};
use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::BuiltinError;

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

    let graph_status =
        status(target_root).map_err(|error| BuiltinError::task_invocation(error.to_string()))?;
    if !graph_status.freshness.usable {
        return Err(BuiltinError::task_invocation(format!(
            "`scan boundary-violations` requires a usable graph index ({})",
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

    let layer_rules = compile_layer_rules(options)?;
    let file_paths: BTreeMap<_, _> = files.into_iter().map(|file| (file.id, file.path)).collect();
    let symbols_by_id: BTreeMap<_, _> = symbols
        .into_iter()
        .map(|symbol| (symbol.id.clone(), symbol))
        .collect();

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
        let Some(target_symbol) = resolve_target_symbol(&edge, &symbols_by_id) else {
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
) -> Option<&'a SymbolRecord> {
    if let Some(target_id) = edge.to_id.as_ref() {
        return symbols_by_id.get(target_id);
    }
    let unresolved = edge.unresolved_target.as_deref()?;
    let normalized = normalize_unresolved_target(unresolved);
    let last_segment = normalized
        .rsplit("::")
        .next()
        .unwrap_or(normalized.as_str());
    let mut candidates = symbols_by_id
        .values()
        .filter(|symbol| {
            symbol.canonical_name == normalized
                || symbol.display_name == normalized
                || symbol.canonical_name == last_segment
                || symbol.display_name == last_segment
                || symbol.canonical_name.ends_with(&format!("::{normalized}"))
        })
        .collect::<Vec<_>>();
    candidates.dedup_by(|left, right| left.id == right.id);
    if candidates.len() == 1 {
        candidates.into_iter().next()
    } else {
        None
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
