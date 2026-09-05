use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::docs_profile::{load_docs_profile_state, DOCS_PROFILE_FINGERPRINT_KEY};
use crate::error::CodeGraphError;
use crate::extractor::{GraphSink, SourceFile};
use crate::json::{
    ExtractorSummaryPayload, GraphCountsPayload, GraphFreshnessPayload, GraphStatusPayload,
};
use crate::model::{
    Confidence, DiagnosticRecord, DiagnosticSeverity, ExtractorRecord, IndexRunRecord, Provenance,
    GRAPH_STORAGE_SCHEMA_VERSION,
};
use crate::phase::{self, GraphPhase};
use crate::registry::ExtractorRegistry;
use crate::storage::{FileScanStateRecord, GraphStore};
use crate::support::{file_record_from_source, sha256_hex};
use crate::walk::ScanEntry;
use crate::GraphId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexReport {
    pub indexed_files: usize,
    pub extractor_count: usize,
    pub stale_paths: Vec<String>,
    pub new_paths: Vec<String>,
    pub changed_paths: Vec<String>,
    pub deleted_paths: Vec<String>,
    pub skipped_paths: Vec<String>,
    pub failed_paths: Vec<String>,
    pub counts: GraphCountsPayload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ScanDelta {
    stale_paths: Vec<String>,
    new_paths: Vec<String>,
    changed_paths: Vec<String>,
    deleted_paths: Vec<String>,
}

pub fn run_index(repo_root: &Path) -> Result<IndexReport, CodeGraphError> {
    crate::refresh::run_index_exclusive(repo_root)
}

pub(crate) fn run_index_unlocked(repo_root: &Path) -> Result<IndexReport, CodeGraphError> {
    let profile_state = load_docs_profile_state(repo_root)?;
    let current_fingerprint = profile_state.fingerprint();
    let store = GraphStore::open(repo_root)?;
    let mut graph_changed = crate::language::markdown::demote_typed_relations(&store)?;
    let existing_states = store.file_scan_state_map()?;
    let stored_extractors = store.extractor_version_map()?;
    let stored_fingerprint = store.metadata_value(DOCS_PROFILE_FINGERPRINT_KEY)?;
    let profile_changed = stored_fingerprint.as_deref() != Some(current_fingerprint.as_str());
    let registry = ExtractorRegistry::for_docs_profile(profile_state.compiled().cloned());
    let current_extractors = extractor_version_map(registry.all());
    phase::enter(GraphPhase::IndexWalk);
    let scan_entries = crate::walk::scan_repo_files(repo_root)?;
    let mut current_states = BTreeMap::new();
    for extractor in registry.all() {
        let record = extractor.extractor_record();
        store.save_extractor(&record)?;
    }

    let mut indexed_paths = BTreeSet::new();
    let mut skipped_paths = Vec::new();

    phase::enter_with_total(GraphPhase::IndexFiles, scan_entries.len());
    for entry in &scan_entries {
        phase::item_done();
        indexed_paths.insert(entry.relative_path.clone());
        let Some(extractor) = registry.for_path(&entry.relative_path) else {
            skipped_paths.push(entry.relative_path.clone());
            continue;
        };
        if let Some(existing_state) = existing_states.get(&entry.relative_path) {
            let extractor_version_matches = current_extractors
                .get(&entry.language_id)
                .zip(stored_extractors.get(&entry.language_id))
                .is_some_and(|(current, stored)| current == stored);
            let unchanged_metadata = existing_state.language_id == entry.language_id
                && existing_state.modified_unix_ms == entry.modified_unix_ms
                && existing_state.byte_size == entry.byte_size;
            if extractor_version_matches
                && unchanged_metadata
                && !profile_requires_markdown_refresh(profile_changed, &entry.language_id)
            {
                current_states.insert(entry.relative_path.clone(), existing_state.clone());
                continue;
            }
        }

        let content = fs::read_to_string(&entry.path).map_err(|error| {
            CodeGraphError::validation(format!("failed to read {}: {error}", entry.path.display()))
        })?;
        let source = SourceFile {
            repo_root: repo_root.to_path_buf(),
            path: entry.path.clone(),
            relative_path: entry.relative_path.clone(),
            language_id: entry.language_id.clone(),
            content,
        };
        let file_record = file_record_from_source(&source)?;
        let file_state = FileScanStateRecord {
            path: source.relative_path.clone(),
            content_hash: file_record.content_hash.clone(),
            language_id: file_record.language_id.clone(),
            modified_unix_ms: entry.modified_unix_ms,
            byte_size: file_record.byte_size,
        };
        let reuse_existing_graph = existing_states
            .get(&entry.relative_path)
            .is_some_and(|state| {
                state.content_hash == file_state.content_hash
                    && state.language_id == file_state.language_id
                    && current_extractors
                        .get(&entry.language_id)
                        .zip(stored_extractors.get(&entry.language_id))
                        .is_some_and(|(current, stored)| current == stored)
            });
        store.save_file_scan_state(&file_state)?;
        current_states.insert(source.relative_path.clone(), file_state);
        if reuse_existing_graph
            && !profile_requires_markdown_refresh(profile_changed, &source.language_id)
        {
            continue;
        }
        graph_changed = true;
        store.delete_file_graph(file_record.id.as_str())?;
        match index_source(
            extractor.extractor_record(),
            extractor,
            &source,
            &file_record,
        ) {
            Ok(output) => {
                store.save_file(&file_record)?;
                for symbol in &output.symbols {
                    store.save_symbol(symbol)?;
                }
                for edge in &output.edges {
                    store.save_edge(edge)?;
                }
                for reference in &output.references {
                    store.save_reference(reference)?;
                }
                for diagnostic in &output.diagnostics {
                    store.save_diagnostic(diagnostic)?;
                }
            }
            Err(error) => {
                let extractor = extractor.extractor_record();
                let diagnostic =
                    extractor_failure_diagnostic(&extractor, &source.relative_path, error)?;
                store.save_diagnostic(&diagnostic)?;
            }
        }
    }

    let new_paths = scan_entries
        .iter()
        .filter(|entry| !existing_states.contains_key(&entry.relative_path))
        .map(|entry| entry.relative_path.clone())
        .collect::<Vec<_>>();
    let changed_paths = scan_entries
        .iter()
        .filter(|entry| {
            existing_states
                .get(&entry.relative_path)
                .is_some_and(|state| {
                    current_states
                        .get(&entry.relative_path)
                        .is_some_and(|current| {
                            state.content_hash != current.content_hash
                                || state.modified_unix_ms != current.modified_unix_ms
                                || state.byte_size != current.byte_size
                        })
                })
        })
        .map(|entry| entry.relative_path.clone())
        .collect::<Vec<_>>();
    let deleted_paths = existing_states
        .keys()
        .filter(|path| !indexed_paths.contains(*path))
        .cloned()
        .collect::<Vec<_>>();
    for path in &deleted_paths {
        let file_id = crate::extractor::file_graph_id(path)?;
        store.delete_file_graph(file_id.as_str())?;
        store.delete_file_scan_state(path)?;
        graph_changed = true;
    }
    if crate::language::markdown::resolve_typed_relations(&store)? {
        graph_changed = true;
    }
    let stale_paths = scan_delta_for_entries(
        &existing_states,
        &stored_extractors,
        &current_extractors,
        &scan_entries,
        profile_changed,
    )?
    .stale_paths;

    let counts = store.counts()?;
    let started_at = unix_epoch_millis_string();
    let finished_at = unix_epoch_millis_string();
    let run = IndexRunRecord {
        id: GraphId::new(format!("run:index:{started_at}"))?,
        repo_root: repo_root.display().to_string(),
        schema_version: GRAPH_STORAGE_SCHEMA_VERSION,
        started_at,
        finished_at: Some(finished_at),
        file_count: counts.files as u64,
        symbol_count: counts.symbols as u64,
        edge_count: counts.edges as u64,
    };
    store.save_index_run(&run)?;
    store.save_metadata(DOCS_PROFILE_FINGERPRINT_KEY, &current_fingerprint)?;
    if graph_changed {
        phase::enter(GraphPhase::SearchIndexRebuild);
        store.refresh_search_index()?;
    }
    crate::git::update_index_stamp(repo_root, &store)?;

    Ok(IndexReport {
        indexed_files: scan_entries.len(),
        extractor_count: registry.all().len(),
        stale_paths,
        new_paths,
        changed_paths,
        deleted_paths,
        skipped_paths,
        failed_paths: store.failed_diagnostic_paths()?,
        counts: store.counts()?,
    })
}

/// `status` with a lazy-refresh pass first: a stale or missing index is
/// rebuilt on demand (the same gate queries use), and any refresh notes are
/// appended to the reported freshness summary. Report-only by default;
/// callers opt in.
pub fn status_with_refresh(repo_root: &Path) -> Result<GraphStatusPayload, CodeGraphError> {
    let store = GraphStore::open(repo_root)?;
    let outcome = crate::refresh::ensure_fresh(repo_root, &store)?;
    let mut payload = status(repo_root)?;
    if !outcome.notes.is_empty() {
        payload.freshness.summary = format!(
            "{} ({})",
            payload.freshness.summary,
            outcome.notes.join("; ")
        );
    }
    Ok(payload)
}

pub fn status(repo_root: &Path) -> Result<GraphStatusPayload, CodeGraphError> {
    let store = GraphStore::open(repo_root)?;
    let profile_state = load_docs_profile_state(repo_root)?;
    let file_states = store.file_scan_state_map()?;
    let registry = ExtractorRegistry::for_docs_profile(profile_state.compiled().cloned());
    let current_entries = crate::walk::scan_repo_files(repo_root)?;
    let current_extractors = extractor_version_map(registry.all());
    let stored_extractors = store.extractor_version_map()?;
    let profile_changed = store
        .metadata_value(DOCS_PROFILE_FINGERPRINT_KEY)?
        .as_deref()
        != Some(profile_state.fingerprint().as_str());
    let scan_delta = scan_delta_for_entries(
        &file_states,
        &stored_extractors,
        &current_extractors,
        &current_entries,
        profile_changed,
    )?;
    let counts = store.counts()?;
    let ready = counts.files > 0;
    let extractors = registry
        .all()
        .iter()
        .map(|extractor| {
            let record = extractor.extractor_record();
            ExtractorSummaryPayload {
                id: record.id.to_string(),
                version: record.version,
                languages: record.language_ids,
                capabilities: record.capabilities,
            }
        })
        .collect::<Vec<_>>();
    Ok(GraphStatusPayload {
        ready,
        index_present: store.paths().db_path.is_file(),
        db_path: store.paths().db_path.display().to_string(),
        storage_schema_version: store.storage_schema_version()?,
        counts,
        freshness: graph_freshness_payload(
            ready,
            store.paths().db_path.is_file(),
            &scan_delta.stale_paths,
            store.failed_diagnostic_paths()?.len(),
        ),
        stale_paths: scan_delta.stale_paths,
        extractors,
        new_paths: scan_delta.new_paths,
        changed_paths: scan_delta.changed_paths,
        deleted_paths: scan_delta.deleted_paths,
        skipped_paths: Vec::new(),
        failed_paths: store.failed_diagnostic_paths()?,
    })
}

pub(crate) fn stale_paths_for_repo(
    repo_root: &Path,
    store: &GraphStore,
) -> Result<Vec<String>, CodeGraphError> {
    phase::enter(GraphPhase::FreshnessScan);
    let profile_state = load_docs_profile_state(repo_root)?;
    let file_states = store.file_scan_state_map()?;
    let current_entries = crate::walk::scan_repo_files(repo_root)?;
    let current_extractors = extractor_version_map(
        ExtractorRegistry::for_docs_profile(profile_state.compiled().cloned()).all(),
    );
    let stored_extractors = store.extractor_version_map()?;
    let profile_changed = store
        .metadata_value(DOCS_PROFILE_FINGERPRINT_KEY)?
        .as_deref()
        != Some(profile_state.fingerprint().as_str());
    Ok(scan_delta_for_entries(
        &file_states,
        &stored_extractors,
        &current_extractors,
        &current_entries,
        profile_changed,
    )?
    .stale_paths)
}

pub(crate) fn graph_freshness_payload(
    ready: bool,
    index_present: bool,
    stale_paths: &[String],
    failed_path_count: usize,
) -> GraphFreshnessPayload {
    let state = if !index_present || !ready {
        "missing-index"
    } else if !stale_paths.is_empty() {
        "refresh-recommended"
    } else if failed_path_count > 0 {
        "degraded"
    } else {
        "ready"
    };

    let summary = match state {
        "missing-index" => "no usable local graph index; run `effigy graph index --json`",
        "refresh-recommended" if failed_path_count > 0 => {
            "graph index is stale and has failed paths; run `effigy graph index --json`"
        }
        "refresh-recommended" => "graph index is stale; run `effigy graph index --json`",
        "degraded" => "graph index is current but has failed paths; results may be incomplete",
        _ => "graph index is current",
    };

    GraphFreshnessPayload {
        state: state.to_owned(),
        summary: summary.to_owned(),
        usable: index_present && ready,
        stale: !stale_paths.is_empty(),
        stale_path_count: stale_paths.len(),
        failed_path_count,
        stale_paths: stale_paths.to_vec(),
    }
}

fn scan_delta_for_entries(
    file_states: &BTreeMap<String, FileScanStateRecord>,
    stored_extractors: &BTreeMap<String, String>,
    current_extractors: &BTreeMap<String, String>,
    current_entries: &[ScanEntry],
    profile_changed: bool,
) -> Result<ScanDelta, CodeGraphError> {
    let mut stale = BTreeSet::new();
    let mut new_paths = Vec::new();
    let mut changed_paths = Vec::new();
    let mut current_paths = BTreeSet::new();
    for entry in current_entries {
        current_paths.insert(entry.relative_path.clone());
        match file_states.get(&entry.relative_path) {
            None => {
                stale.insert(entry.relative_path.clone());
                new_paths.push(entry.relative_path.clone());
            }
            Some(state) => {
                let metadata_changed = state.modified_unix_ms != entry.modified_unix_ms
                    || state.byte_size != entry.byte_size;
                if metadata_changed {
                    changed_paths.push(entry.relative_path.clone());
                }
                let extractor_version_changed = current_extractors
                    .get(&entry.language_id)
                    .zip(stored_extractors.get(&entry.language_id))
                    .is_some_and(|(current_version, stored_version)| {
                        stored_version != current_version
                    });
                if extractor_version_changed
                    || profile_requires_markdown_refresh(profile_changed, &entry.language_id)
                {
                    stale.insert(entry.relative_path.clone());
                    continue;
                }
                if !metadata_changed {
                    continue;
                }
                let content_hash = sha256_hex(fs::read(&entry.path)?.as_slice());
                if state.content_hash != content_hash {
                    stale.insert(entry.relative_path.clone());
                }
            }
        }
    }
    let mut deleted_paths = Vec::new();
    for path in file_states.keys() {
        if !current_paths.contains(path) {
            stale.insert(path.clone());
            deleted_paths.push(path.clone());
        }
    }
    Ok(ScanDelta {
        stale_paths: stale.into_iter().collect(),
        new_paths,
        changed_paths,
        deleted_paths,
    })
}

fn profile_requires_markdown_refresh(profile_changed: bool, language_id: &str) -> bool {
    profile_changed && language_id == "markdown"
}

fn index_source(
    extractor_record: ExtractorRecord,
    extractor: &dyn crate::extractor::LanguageIndexer,
    source: &SourceFile,
    file_record: &crate::model::FileRecord,
) -> Result<crate::extractor::ExtractorOutput, CodeGraphError> {
    let mut sink = GraphSink::default();
    extractor.extract(source, file_record, &mut sink)?;
    let output = sink.into_output();
    output.validate()?;
    let _ = extractor_record;
    Ok(output)
}

fn extractor_failure_diagnostic(
    extractor: &ExtractorRecord,
    path: &str,
    error: CodeGraphError,
) -> Result<DiagnosticRecord, CodeGraphError> {
    Ok(DiagnosticRecord {
        id: GraphId::new(format!("diag:extract:{path}"))?,
        severity: DiagnosticSeverity::Error,
        message: error.to_string(),
        file_id: Some(GraphId::new(format!("file:{path}"))?),
        span: None,
        provenance: Provenance {
            extractor_id: extractor.id.clone(),
            extractor_version: extractor.version.clone(),
            source_path: path.to_owned(),
            confidence: Confidence::Exact,
            detail: Some("extractor".to_owned()),
        },
    })
}

fn extractor_version_map(
    extractors: &[Box<dyn crate::extractor::LanguageIndexer>],
) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for extractor in extractors {
        let record = extractor.extractor_record();
        for language in record.language_ids {
            map.insert(language, record.version.clone());
        }
    }
    map
}

fn unix_epoch_millis_string() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    duration.as_millis().to_string()
}
