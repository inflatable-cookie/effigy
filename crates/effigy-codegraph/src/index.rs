use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::error::CodeGraphError;
use crate::extractor::{GraphSink, SourceFile};
use crate::json::{ExtractorSummaryPayload, GraphCountsPayload, GraphStatusPayload};
use crate::model::{
    Confidence, DiagnosticRecord, DiagnosticSeverity, ExtractorRecord, IndexRunRecord, Provenance,
    GRAPH_STORAGE_SCHEMA_VERSION,
};
use crate::registry::ExtractorRegistry;
use crate::storage::{FileScanStateRecord, GraphStore};
use crate::support::{file_record_from_source, sha256_hex};
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

pub fn run_index(repo_root: &Path) -> Result<IndexReport, CodeGraphError> {
    let registry = ExtractorRegistry::builtins();
    let store = GraphStore::open(repo_root)?;
    let existing_states = store.file_scan_state_map()?;
    let scan_entries = crate::walk::scan_repo_files(repo_root)?;
    let mut current_states = BTreeMap::new();

    store.clear_graph_data()?;
    for extractor in registry.all() {
        let record = extractor.extractor_record();
        store.save_extractor(&record)?;
    }

    let mut indexed_paths = BTreeSet::new();
    let mut skipped_paths = Vec::new();
    let mut failed_paths = Vec::new();
    let mut diagnostics = Vec::new();

    for entry in &scan_entries {
        indexed_paths.insert(entry.relative_path.clone());
        let Some(extractor) = registry.for_path(&entry.relative_path) else {
            skipped_paths.push(entry.relative_path.clone());
            continue;
        };
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
        store.save_file_scan_state(&file_state)?;
        current_states.insert(source.relative_path.clone(), file_state);
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
                diagnostics.extend(output.diagnostics);
            }
            Err(error) => {
                failed_paths.push(source.relative_path.clone());
                let extractor = extractor.extractor_record();
                let diagnostic =
                    extractor_failure_diagnostic(&extractor, &source.relative_path, error)?;
                store.save_diagnostic(&diagnostic)?;
                diagnostics.push(diagnostic);
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
    let stale_paths = stale_paths_for_repo(repo_root, &store)?;

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
    store.refresh_search_index()?;

    Ok(IndexReport {
        indexed_files: scan_entries.len(),
        extractor_count: registry.all().len(),
        stale_paths,
        new_paths,
        changed_paths,
        deleted_paths,
        skipped_paths,
        failed_paths,
        counts: store.counts()?,
    })
}

pub fn status(repo_root: &Path) -> Result<GraphStatusPayload, CodeGraphError> {
    let store = GraphStore::open(repo_root)?;
    let registry = ExtractorRegistry::builtins();
    let stale_paths = stale_paths_for_repo(repo_root, &store)?;
    let counts = store.counts()?;
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
    let file_states = store.file_scan_state_map()?;
    Ok(GraphStatusPayload {
        ready: counts.files > 0,
        index_present: store.paths().db_path.is_file(),
        db_path: store.paths().db_path.display().to_string(),
        storage_schema_version: store.storage_schema_version()?,
        counts,
        stale_paths,
        extractors,
        new_paths: current_new_paths(repo_root, &file_states)?,
        changed_paths: current_changed_paths(repo_root, &file_states)?,
        deleted_paths: current_deleted_paths(repo_root, &file_states)?,
        skipped_paths: Vec::new(),
        failed_paths: store.failed_diagnostic_paths()?,
    })
}

pub(crate) fn stale_paths_for_repo(
    repo_root: &Path,
    store: &GraphStore,
) -> Result<Vec<String>, CodeGraphError> {
    let file_states = store.file_scan_state_map()?;
    let current_entries = crate::walk::scan_repo_files(repo_root)?;
    let current_extractors = extractor_version_map(ExtractorRegistry::builtins().all());
    let stored_extractors = store.extractor_version_map()?;
    let mut stale = BTreeSet::new();
    for entry in &current_entries {
        match file_states.get(&entry.relative_path) {
            None => {
                stale.insert(entry.relative_path.clone());
            }
            Some(state) => {
                let content_hash = sha256_hex(fs::read(&entry.path)?.as_slice());
                if state.content_hash != content_hash
                    || state.modified_unix_ms != entry.modified_unix_ms
                    || state.byte_size != entry.byte_size
                {
                    stale.insert(entry.relative_path.clone());
                }
                if let Some(current_version) = current_extractors.get(&entry.language_id) {
                    if stored_extractors
                        .get(&entry.language_id)
                        .is_some_and(|stored_version| stored_version != current_version)
                    {
                        stale.insert(entry.relative_path.clone());
                    }
                }
            }
        }
    }
    for path in file_states.keys() {
        if !current_entries
            .iter()
            .any(|entry| entry.relative_path == *path)
        {
            stale.insert(path.clone());
        }
    }
    Ok(stale.into_iter().collect())
}

fn current_new_paths(
    repo_root: &Path,
    file_states: &BTreeMap<String, FileScanStateRecord>,
) -> Result<Vec<String>, CodeGraphError> {
    Ok(crate::walk::scan_repo_files(repo_root)?
        .into_iter()
        .filter(|entry| !file_states.contains_key(&entry.relative_path))
        .map(|entry| entry.relative_path)
        .collect())
}

fn current_changed_paths(
    repo_root: &Path,
    file_states: &BTreeMap<String, FileScanStateRecord>,
) -> Result<Vec<String>, CodeGraphError> {
    let mut changed = Vec::new();
    for entry in crate::walk::scan_repo_files(repo_root)? {
        if let Some(state) = file_states.get(&entry.relative_path) {
            let content_hash = sha256_hex(fs::read(&entry.path)?.as_slice());
            if state.content_hash != content_hash
                || state.modified_unix_ms != entry.modified_unix_ms
                || state.byte_size != entry.byte_size
            {
                changed.push(entry.relative_path);
            }
        }
    }
    Ok(changed)
}

fn current_deleted_paths(
    repo_root: &Path,
    file_states: &BTreeMap<String, FileScanStateRecord>,
) -> Result<Vec<String>, CodeGraphError> {
    let current_paths = crate::walk::scan_repo_files(repo_root)?
        .into_iter()
        .map(|entry| entry.relative_path)
        .collect::<BTreeSet<_>>();
    Ok(file_states
        .keys()
        .filter(|path| !current_paths.contains(*path))
        .cloned()
        .collect())
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
