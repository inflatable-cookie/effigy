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
use crate::scope::GraphScope;
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

/// Index every supported file in this checkout.
pub fn run_index(repo_root: &Path) -> Result<IndexReport, CodeGraphError> {
    run_index_in_scope(&repo_scope(repo_root)?)
}

/// Build or refresh one selected graph scope.
pub fn run_index_in_scope(scope: &GraphScope) -> Result<IndexReport, CodeGraphError> {
    crate::refresh::run_index_exclusive(scope)
}

pub(crate) fn run_index_unlocked_in_scope(
    scope: &GraphScope,
) -> Result<IndexReport, CodeGraphError> {
    let repo_root = scope.workspace_root();
    crate::deadline::check()?;
    let profile_state = load_docs_profile_state(repo_root)?;
    let current_fingerprint = profile_state.fingerprint();
    let store = GraphStore::open_for_scope(scope)?;
    for name in [
        "graph.complete.db",
        "graph.complete.db-wal",
        "graph.complete.db-shm",
    ] {
        match fs::remove_file(store.paths().graph_dir.join(name)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let existing_states = store.file_scan_state_map()?;
    let registry = ExtractorRegistry::for_docs_profile(profile_state.compiled().cloned());
    let current_extractors = extractor_version_map(registry.all());
    let keys = file_freshness_keys(&current_extractors, &current_fingerprint);
    phase::enter(GraphPhase::IndexWalk);
    let scan_entries = crate::walk::scan_repo_files_in_scope(scope)?;
    let delta = scan_delta_for_entries(&existing_states, &keys, &scan_entries)?;
    // Clear the advisory git stamp before mutation. An interruption must never
    // let a clean HEAD hide an unfinished refresh or a profile upgrade.
    crate::git::clear_index_stamp(scope, &store)?;
    store.save_metadata(&relations_pending_key(scope), "true")?;
    let total = scan_entries.len() + delta.deleted_paths.len();
    save_progress(&store, 0, total)?;
    phase::enter_with_total(GraphPhase::IndexFiles, total);
    let mut done = 0;
    let mut skipped_paths = Vec::new();
    for entry in &scan_entries {
        crate::deadline::check()?;
        let Some(extractor) = registry.for_path(&entry.relative_path) else {
            skipped_paths.push(entry.relative_path.clone());
            continue;
        };
        let freshness_key = keys.get(&entry.language_id).cloned().unwrap_or_default();
        if existing_states
            .get(&entry.relative_path)
            .is_some_and(|state| {
                state.freshness_key == freshness_key
                    && state.language_id == entry.language_id
                    && state.modified_unix_ms == entry.modified_unix_ms
                    && state.byte_size == entry.byte_size
            })
        {
            done += 1;
            phase::item_done();
            continue;
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
            freshness_key,
        };
        // Parse before taking the write transaction. No destructive work has
        // happened if parsing or the deadline fails here.
        let output = index_source(
            extractor.extractor_record(),
            extractor,
            &source,
            &file_record,
        );
        crate::deadline::check()?;
        let transaction = store.transaction()?;
        store.delete_file_graph(file_record.id.as_str())?;
        #[cfg(test)]
        crate::tests::file_transactions::boundary(&source.relative_path, "deleted");
        store.save_file(&file_record)?;
        match output {
            Ok(output) => {
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
                let diagnostic = extractor_failure_diagnostic(
                    &extractor.extractor_record(),
                    &source.relative_path,
                    error,
                )?;
                store.save_diagnostic(&diagnostic)?;
            }
        }
        #[cfg(test)]
        crate::tests::file_transactions::boundary(&source.relative_path, "graph");
        store.refresh_file_search(&file_record, &source.content)?;
        #[cfg(test)]
        crate::tests::file_transactions::boundary(&source.relative_path, "search");
        store.save_file_scan_state(&file_state)?;
        #[cfg(test)]
        crate::tests::file_transactions::boundary(&source.relative_path, "state");
        save_progress(&store, done + 1, total)?;
        transaction.commit()?;
        done += 1;
        phase::item_done();
        #[cfg(test)]
        crate::tests::file_transactions::boundary(&source.relative_path, "committed");
    }
    for path in &delta.deleted_paths {
        crate::deadline::check()?;
        let transaction = store.transaction()?;
        let file_id = crate::extractor::file_graph_id(path)?;
        store.delete_file_graph(file_id.as_str())?;
        #[cfg(test)]
        crate::tests::file_transactions::boundary(path, "delete-graph");
        store.delete_file_scan_state(path)?;
        #[cfg(test)]
        crate::tests::file_transactions::boundary(path, "delete-state");
        save_progress(&store, done + 1, total)?;
        transaction.commit()?;
        #[cfg(test)]
        crate::tests::file_transactions::boundary(path, "delete-committed");
        done += 1;
        phase::item_done();
    }
    crate::language::markdown::resolve_typed_relations(&store, scope)?;
    crate::deadline::check()?;
    let transaction = store.transaction()?;
    for extractor in registry.all() {
        store.save_extractor(&extractor.extractor_record())?;
    }
    let counts = store.counts()?;
    let started_at = unix_epoch_millis_string();
    let run = IndexRunRecord {
        id: GraphId::new(index_run_id(scope, &started_at))?,
        repo_root: repo_root.display().to_string(),
        schema_version: GRAPH_STORAGE_SCHEMA_VERSION,
        started_at,
        finished_at: Some(unix_epoch_millis_string()),
        file_count: counts.files as u64,
        symbol_count: counts.symbols as u64,
        edge_count: counts.edges as u64,
    };
    store.save_index_run(&run)?;
    store.save_metadata(DOCS_PROFILE_FINGERPRINT_KEY, &current_fingerprint)?;
    store.delete_metadata(&relations_pending_key(scope))?;
    transaction.commit()?;
    crate::git::update_index_stamp(scope, &store)?;
    Ok(IndexReport {
        indexed_files: scan_entries.len(),
        extractor_count: registry.all().len(),
        stale_paths: delta.stale_paths,
        new_paths: delta.new_paths,
        changed_paths: delta.changed_paths,
        deleted_paths: delta.deleted_paths,
        skipped_paths,
        failed_paths: store.failed_diagnostic_paths()?,
        counts,
    })
}

fn save_progress(store: &GraphStore, done: usize, total: usize) -> Result<(), CodeGraphError> {
    store.save_metadata("refresh_progress", &format!("{done}/{total}"))
}

fn relations_pending_key(scope: &GraphScope) -> String {
    format!("relations_pending:{}", scope.key())
}

fn file_freshness_keys(
    extractors: &BTreeMap<String, String>,
    profile: &str,
) -> BTreeMap<String, String> {
    extractors
        .iter()
        .map(|(language, version)| {
            let profile = if language == "markdown" { profile } else { "" };
            (
                language.clone(),
                sha256_hex(format!("{version}|{profile}").as_bytes()),
            )
        })
        .collect()
}

/// Compatibility spelling for report-only status. The old refresh request
/// warns for one release and never rebuilds a missing or stale index.
pub fn status_with_refresh(repo_root: &Path) -> Result<GraphStatusPayload, CodeGraphError> {
    status_in_scope(&repo_scope(repo_root)?, true)
}

/// Report-only status. The legacy `refresh` flag warns for one release.
pub fn status_in_scope(
    scope: &GraphScope,
    refresh: bool,
) -> Result<GraphStatusPayload, CodeGraphError> {
    // Compatibility spelling only: status is always Observe under R2.
    if refresh {
        eprintln!(
            "[graph] --refresh on status is retired; run `effigy graph index` to update the index"
        );
    }
    status_report(scope)
}

pub fn status(repo_root: &Path) -> Result<GraphStatusPayload, CodeGraphError> {
    status_report(&repo_scope(repo_root)?)
}

fn status_report(scope: &GraphScope) -> Result<GraphStatusPayload, CodeGraphError> {
    let store = GraphStore::open_read_only(scope)?;
    if let Some(store) = store.as_ref() {
        store.begin_read()?;
    }
    let profile_state = load_docs_profile_state(scope.workspace_root())?;
    let file_states = store
        .as_ref()
        .map(|store| store.file_scan_state_map())
        .transpose()?
        .unwrap_or_default();
    let registry = ExtractorRegistry::for_docs_profile(profile_state.compiled().cloned());
    let current_entries = crate::walk::scan_repo_files_in_scope(scope)?;
    let current_extractors = extractor_version_map(registry.all());
    let keys = file_freshness_keys(&current_extractors, &profile_state.fingerprint());
    let mut scan_delta = scan_delta_for_entries(&file_states, &keys, &current_entries)?;
    if let Some(store) = store.as_ref() {
        add_pending_relations(scope, store, &current_entries, &mut scan_delta.stale_paths)?;
    }
    let counts = store
        .as_ref()
        .map(|store| store.counts())
        .transpose()?
        .unwrap_or(GraphCountsPayload {
            files: 0,
            symbols: 0,
            edges: 0,
            references: 0,
            diagnostics: 0,
            extractors: 0,
            index_runs: 0,
        });
    let failed_paths = store
        .as_ref()
        .map(|store| store.failed_diagnostic_paths())
        .transpose()?
        .unwrap_or_default();
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
    let mut freshness = graph_freshness_payload(
        ready,
        scope.paths().db_path.is_file(),
        &scan_delta.stale_paths,
        failed_paths.len(),
    );
    let lock = crate::refresh::inspect_refresh_lock(&scope.paths().refresh_lock_path);
    if GraphStore::has_wal(scope) {
        freshness.state = "refresh-recommended".to_owned();
        freshness.stale = true;
        freshness.summary = "graph stale (refresh interrupted or in progress); run `effigy graph index` to migrate WAL storage".into();
    } else if lock.unknown {
        freshness.state = "refresh-recommended".into();
        freshness.stale = true;
        freshness.summary =
            "graph refresh gate is unobservable; run `effigy graph index` on the host".into();
    } else if lock.held {
        let progress = store
            .as_ref()
            .map(|store| store.metadata_value("refresh_progress"))
            .transpose()?
            .flatten()
            .unwrap_or_else(|| "0/?".to_owned());
        freshness.state = "refreshing".to_owned();
        freshness.stale = true;
        freshness.lock = Some(lock.payload());
        freshness.summary =
            format!("graph refreshing ({progress}); committed files remain queryable");
    }
    Ok(GraphStatusPayload {
        ready,
        index_present: scope.paths().db_path.is_file(),
        db_path: scope.paths().db_path.display().to_string(),
        storage_schema_version: store
            .as_ref()
            .map(GraphStore::storage_schema_version)
            .transpose()?
            .unwrap_or(0),
        counts,
        freshness,
        stale_paths: scan_delta.stale_paths,
        extractors,
        new_paths: scan_delta.new_paths,
        changed_paths: scan_delta.changed_paths,
        deleted_paths: scan_delta.deleted_paths,
        skipped_paths: Vec::new(),
        failed_paths,
    })
}

pub(crate) fn stale_paths_in_scope(
    scope: &GraphScope,
    store: &GraphStore,
) -> Result<Vec<String>, CodeGraphError> {
    phase::enter(GraphPhase::FreshnessScan);
    let profile_state = load_docs_profile_state(scope.workspace_root())?;
    let file_states = store.file_scan_state_map()?;
    let current_entries = crate::walk::scan_repo_files_in_scope(scope)?;
    let current_extractors = extractor_version_map(
        ExtractorRegistry::for_docs_profile(profile_state.compiled().cloned()).all(),
    );
    let keys = file_freshness_keys(&current_extractors, &profile_state.fingerprint());
    let mut stale = scan_delta_for_entries(&file_states, &keys, &current_entries)?.stale_paths;
    add_pending_relations(scope, store, &current_entries, &mut stale)?;
    Ok(stale)
}

fn add_pending_relations(
    scope: &GraphScope,
    store: &GraphStore,
    entries: &[ScanEntry],
    stale: &mut Vec<String>,
) -> Result<(), CodeGraphError> {
    if store
        .metadata_value(&relations_pending_key(scope))?
        .is_some()
    {
        stale.extend(
            entries
                .iter()
                .filter(|entry| entry.language_id == "markdown")
                .map(|entry| entry.relative_path.clone()),
        );
        stale.sort();
        stale.dedup();
    }
    Ok(())
}

fn repo_scope(repo_root: &Path) -> Result<GraphScope, CodeGraphError> {
    GraphScope::repo_root(repo_root)
}

fn index_run_id(_scope: &GraphScope, started_at: &str) -> String {
    format!("run:index:{started_at}")
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
        lock: None,
    }
}

/// Freshness for a deliberate read of committed file generations.
pub(crate) fn stale_index_freshness_payload(
    stale_paths: &[String],
    failed_path_count: usize,
    lock: Option<crate::json::GraphLockPayload>,
) -> GraphFreshnessPayload {
    let mut summary =
        "serving committed graph files (read-only, not current); run `effigy graph index --json`"
            .to_owned();
    if let Some(lock) = lock.as_ref() {
        summary.push_str(&format!("; {}", describe_lock(lock)));
    }
    GraphFreshnessPayload {
        state: "stale-index".to_owned(),
        summary,
        usable: true,
        stale: true,
        stale_path_count: stale_paths.len(),
        failed_path_count,
        stale_paths: stale_paths.to_vec(),
        lock,
    }
}

fn describe_lock(lock: &crate::json::GraphLockPayload) -> String {
    match (lock.pid, lock.age_ms, lock.stale_holder) {
        (Some(pid), Some(age_ms), true) => {
            format!("refresh lock holder pid {pid} is stale (age {age_ms}ms)")
        }
        (Some(pid), Some(age_ms), false) => {
            format!("refresh lock held by pid {pid} for {age_ms}ms")
        }
        (Some(pid), None, true) => format!("refresh lock holder pid {pid} is stale"),
        (Some(pid), None, false) => format!("refresh lock held by pid {pid}"),
        (_, Some(age_ms), _) => format!("refresh lock held for {age_ms}ms"),
        _ => "refresh lock is held".to_owned(),
    }
}

fn scan_delta_for_entries(
    file_states: &BTreeMap<String, FileScanStateRecord>,
    freshness_keys: &BTreeMap<String, String>,
    current_entries: &[ScanEntry],
) -> Result<ScanDelta, CodeGraphError> {
    let mut stale = BTreeSet::new();
    let mut new_paths = Vec::new();
    let mut changed_paths = Vec::new();
    let mut current_paths = BTreeSet::new();
    for entry in current_entries {
        crate::deadline::check()?;
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
                if freshness_keys.get(&entry.language_id) != Some(&state.freshness_key)
                    || state.language_id != entry.language_id
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
    let encoded = crate::support::encode_graph_path(path);
    Ok(DiagnosticRecord {
        id: GraphId::new(format!("diag:extract:{encoded}"))?,
        severity: DiagnosticSeverity::Error,
        message: error.to_string(),
        file_id: Some(crate::extractor::file_graph_id(path)?),
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
