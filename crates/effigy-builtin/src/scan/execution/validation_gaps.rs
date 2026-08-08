use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use effigy_codegraph::json::{GraphAffectedFilePayload, GraphAffectedTaskPayload};
use effigy_codegraph::model::{Confidence, FileIndexStatus};
use effigy_codegraph::{affected, GraphId};
use effigy_scan::{
    ValidationGapConfidence, ValidationGapFinding, ValidationGapFindingKind,
    ValidationGapScanOptions, ValidationGapScanResult, ValidationGapSeverity,
    ValidationGapTestTarget,
};

use crate::BuiltinError;

use super::graph_helpers::{
    classify_file_role, compile_globs, first_symbol_line, open_fresh_graph_store,
    supported_language_map, FileRole, FileRoleOptions,
};

pub(super) fn run_validation_gap_scan(
    target_root: &Path,
    options: &ValidationGapScanOptions,
    changed_paths: &[String],
    read_stdin: bool,
) -> Result<ValidationGapScanResult, BuiltinError> {
    options
        .validate()
        .map_err(|error| BuiltinError::task_invocation(error.to_string()))?;

    let store = open_fresh_graph_store(target_root, "validation-gaps")?;
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
    let allow_paths = compile_globs("scan.validation_gaps.allow_paths", &options.allow_paths)?;
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
    }

    for edge in &edges {
        if edge.kind == "contains" {
            continue;
        }
        if !options.include_heuristic && edge.provenance.confidence == Confidence::Heuristic {
            continue;
        }
        if let Some(source_symbol) = symbol_by_id.get(&edge.from_id) {
            let stats = file_stats.entry(source_symbol.file_id.clone()).or_default();
            stats.outbound_edges += 1;
        } else if file_by_id.contains_key(&edge.from_id) {
            file_stats
                .entry(edge.from_id.clone())
                .or_default()
                .outbound_edges += 1;
        }
        if let Some(target_id) = edge.to_id.as_ref() {
            if let Some(target_symbol) = symbol_by_id.get(target_id) {
                file_stats
                    .entry(target_symbol.file_id.clone())
                    .or_default()
                    .inbound_edges += 1;
            } else if file_by_id.contains_key(target_id) {
                file_stats
                    .entry(target_id.clone())
                    .or_default()
                    .inbound_edges += 1;
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
                file_stats
                    .entry(target_symbol.file_id.clone())
                    .or_default()
                    .inbound_references += 1;
            }
        }
    }

    let mut checked_files = 0usize;
    let mut skipped_allowlisted_paths = 0usize;
    let mut skipped_non_implementation_files = 0usize;
    let mut skipped_unsupported_language_files = 0usize;
    let mut candidates = Vec::new();

    for (file_id, file) in &file_by_id {
        if file.status != FileIndexStatus::Indexed {
            continue;
        }
        let role = classify_file_role(
            &file.path,
            &file.language_id,
            FileRoleOptions::validation_gaps(),
        );
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
        candidates.push(CandidateFile {
            path: file.path.clone(),
            line: first_symbol_line(&symbol_ids, &symbol_by_id),
            language_id: file.language_id.clone(),
            stats: file_stats.get(file_id).cloned().unwrap_or_default(),
        });
    }

    let changed_paths = collect_changed_paths(changed_paths, read_stdin)?;
    let mode = if changed_paths.is_empty() {
        "hotspots"
    } else {
        "changed-paths"
    }
    .to_owned();
    let candidate_by_path = candidates
        .iter()
        .map(|candidate| (candidate.path.clone(), candidate))
        .collect::<BTreeMap<_, _>>();
    let mut likely_test_files = Vec::new();
    let mut likely_test_tasks = Vec::new();
    let mut seen_file_targets = BTreeSet::new();
    let mut seen_task_targets = BTreeSet::new();
    let mut findings = Vec::new();

    if changed_paths.is_empty() {
        for candidate in candidates
            .iter()
            .filter(|candidate| candidate.stats.connectivity() >= options.hotspot_threshold)
        {
            let affected = affected(
                target_root,
                std::slice::from_ref(&candidate.path),
                options.affected_depth,
                Some(25),
            )
            .map_err(|error| BuiltinError::task_invocation(error.to_string()))?;
            if affected.likely_test_files.is_empty() && affected.likely_test_tasks.is_empty() {
                findings.push(ValidationGapFinding {
                    kind: ValidationGapFindingKind::HotspotWithoutNearbyTests,
                    path: candidate.path.clone(),
                    line: candidate.line,
                    language_id: candidate.language_id.clone(),
                    confidence: ValidationGapConfidence::High,
                    severity: ValidationGapSeverity::Warning,
                    reason: format!(
                        "connectivity {} meets hotspot threshold {} but no nearby test files or tasks were discovered",
                        candidate.stats.connectivity(),
                        options.hotspot_threshold
                    ),
                    connectivity: candidate.stats.connectivity(),
                    inbound_edges: candidate.stats.inbound_edges,
                    outbound_edges: candidate.stats.outbound_edges,
                    inbound_references: candidate.stats.inbound_references,
                    outbound_references: candidate.stats.outbound_references,
                });
            }
        }
    } else {
        for changed_path in &changed_paths {
            let Some(candidate) = candidate_by_path.get(changed_path) else {
                continue;
            };
            let affected = affected(
                target_root,
                std::slice::from_ref(changed_path),
                options.affected_depth,
                Some(25),
            )
            .map_err(|error| BuiltinError::task_invocation(error.to_string()))?;
            merge_file_targets(
                &mut likely_test_files,
                &mut seen_file_targets,
                &affected.likely_test_files,
            );
            merge_task_targets(
                &mut likely_test_tasks,
                &mut seen_task_targets,
                &affected.likely_test_tasks,
            );
            if affected.likely_test_files.is_empty() && affected.likely_test_tasks.is_empty() {
                findings.push(ValidationGapFinding {
                    kind: ValidationGapFindingKind::ChangedOwnerWithoutTestTarget,
                    path: candidate.path.clone(),
                    line: candidate.line,
                    language_id: candidate.language_id.clone(),
                    confidence: ValidationGapConfidence::High,
                    severity: ValidationGapSeverity::High,
                    reason:
                        "changed owner has no nearby test files or tasks in the current graph slice"
                            .to_owned(),
                    connectivity: candidate.stats.connectivity(),
                    inbound_edges: candidate.stats.inbound_edges,
                    outbound_edges: candidate.stats.outbound_edges,
                    inbound_references: candidate.stats.inbound_references,
                    outbound_references: candidate.stats.outbound_references,
                });
            }
        }
    }

    findings.sort_by(|left, right| left.path.cmp(&right.path).then(left.line.cmp(&right.line)));

    Ok(ValidationGapScanResult {
        root: target_root.display().to_string(),
        mode,
        hotspot_threshold: options.hotspot_threshold,
        affected_depth: options.affected_depth,
        changed_paths,
        checked_files,
        skipped_allowlisted_paths,
        skipped_non_implementation_files,
        skipped_unsupported_language_files,
        likely_test_files,
        likely_test_tasks,
        findings,
    })
}

#[derive(Debug, Clone)]
struct CandidateFile {
    path: String,
    line: usize,
    language_id: String,
    stats: FileStats,
}

#[derive(Debug, Clone, Default)]
struct FileStats {
    inbound_edges: usize,
    outbound_edges: usize,
    inbound_references: usize,
    outbound_references: usize,
}

impl FileStats {
    fn connectivity(&self) -> usize {
        self.inbound_edges
            + self.outbound_edges
            + self.inbound_references
            + self.outbound_references
    }
}

fn collect_changed_paths(
    requested_paths: &[String],
    read_stdin: bool,
) -> Result<Vec<String>, BuiltinError> {
    let mut changed_paths = requested_paths.to_vec();
    if read_stdin {
        changed_paths.extend(read_stdin_paths()?);
    }
    Ok(changed_paths
        .into_iter()
        .map(|path| path.trim().to_owned())
        .filter(|path| !path.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect())
}

fn read_stdin_paths() -> Result<Vec<String>, BuiltinError> {
    use std::io::Read;

    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| {
            BuiltinError::task_invocation(format!(
                "failed to read stdin for `scan validation-gaps`: {error}"
            ))
        })?;
    Ok(input
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

fn merge_file_targets(
    out: &mut Vec<ValidationGapTestTarget>,
    seen: &mut BTreeSet<String>,
    targets: &[GraphAffectedFilePayload],
) {
    for target in targets {
        if seen.insert(target.path.clone()) {
            out.push(ValidationGapTestTarget {
                name: target.path.clone(),
                kind: "file".to_owned(),
                path: target.path.clone(),
                confidence: target.confidence.clone(),
                reasons: target.reasons.clone(),
            });
        }
    }
}

fn merge_task_targets(
    out: &mut Vec<ValidationGapTestTarget>,
    seen: &mut BTreeSet<String>,
    targets: &[GraphAffectedTaskPayload],
) {
    for target in targets {
        let key = format!("{}:{}", target.path, target.name);
        if seen.insert(key) {
            out.push(ValidationGapTestTarget {
                name: target.name.clone(),
                kind: target.kind.clone(),
                path: target.path.clone(),
                confidence: target.confidence.clone(),
                reasons: target.reasons.clone(),
            });
        }
    }
}
