//! Foreground filesystem watch support for `effigy graph watch`.
//!
//! Watch mode is intentionally narrow:
//! - foreground only
//! - debounce-driven
//! - built on the normal incremental indexer
//! - explicit dirty/reconcile fallback when the watch backend cannot provide a
//!   trustworthy changed-path set

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::error::CodeGraphError;
use crate::json::{GraphIndexPayload, GraphWatchEventPayload};
use crate::run_index;
use crate::support::normalize_rel_path;
use crate::walk::should_skip_path;

/// Runtime options for foreground graph watch mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphWatchOptions {
    /// Debounce window in milliseconds before a refresh batch is flushed.
    pub debounce_ms: u64,
}

/// Typed watch event emitted by `watch_repo`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphWatchEvent {
    /// Public JSON payload for the event.
    pub payload: GraphWatchEventPayload,
}

#[derive(Debug, Default)]
struct PendingWatchState {
    changed_paths: BTreeSet<String>,
    dirty_notes: BTreeSet<String>,
}

/// Watch `repo_root` for filesystem changes and emit graph refresh events.
///
/// This is the engine behind `effigy graph watch`. It never detaches, never
/// mutates hidden background state, and falls back to dirty reconcile mode when
/// the watcher backend cannot provide a reliable path-level change set.
pub fn watch_repo<F>(
    repo_root: &Path,
    options: &GraphWatchOptions,
    mut emit: F,
) -> Result<(), CodeGraphError>
where
    F: FnMut(GraphWatchEvent) -> Result<(), CodeGraphError>,
{
    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = build_watcher(tx)?;
    watcher
        .watch(repo_root, RecursiveMode::Recursive)
        .map_err(|error| {
            CodeGraphError::validation(format!("graph watch start failed: {error}"))
        })?;

    emit(GraphWatchEvent {
        payload: GraphWatchEventPayload {
            kind: "started".to_owned(),
            debounce_ms: options.debounce_ms,
            changed_paths: Vec::new(),
            dirty: false,
            refresh_duration_ms: None,
            index: None,
            notes: vec![format!(
                "watching {} with {}ms debounce",
                repo_root.display(),
                options.debounce_ms
            )],
        },
    })?;

    let debounce = Duration::from_millis(options.debounce_ms);
    let mut pending = PendingWatchState::default();

    loop {
        let event = rx.recv().map_err(|error| {
            CodeGraphError::validation(format!("graph watch channel closed: {error}"))
        })?;
        collect_watch_event(
            repo_root,
            event,
            options.debounce_ms,
            &mut pending,
            &mut emit,
        )?;
        if pending.is_empty() {
            continue;
        }
        let mut quiet_deadline = Instant::now() + debounce;
        loop {
            let remaining = quiet_deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(remaining) {
                Ok(event) => {
                    collect_watch_event(
                        repo_root,
                        event,
                        options.debounce_ms,
                        &mut pending,
                        &mut emit,
                    )?;
                    if !pending.is_empty() {
                        quiet_deadline = Instant::now() + debounce;
                    }
                }
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(CodeGraphError::validation(
                        "graph watch channel disconnected".to_owned(),
                    ));
                }
            }
        }

        let payload = flush_watch_batch(repo_root, options.debounce_ms, &mut pending)?;
        emit(GraphWatchEvent { payload })?;
    }
}

fn build_watcher(
    tx: mpsc::Sender<notify::Result<Event>>,
) -> Result<RecommendedWatcher, CodeGraphError> {
    notify::recommended_watcher(move |event| {
        let _ = tx.send(event);
    })
    .and_then(|mut watcher| {
        watcher.configure(Config::default())?;
        Ok(watcher)
    })
    .map_err(|error| CodeGraphError::validation(format!("graph watcher init failed: {error}")))
}

fn collect_watch_event<F>(
    repo_root: &Path,
    event: notify::Result<Event>,
    debounce_ms: u64,
    pending: &mut PendingWatchState,
    emit: &mut F,
) -> Result<(), CodeGraphError>
where
    F: FnMut(GraphWatchEvent) -> Result<(), CodeGraphError>,
{
    match event {
        Ok(event) => {
            if !should_trigger_refresh(&event.kind) {
                return Ok(());
            }
            let mut saw_repo_relative_path = false;
            let mut added_tracked_path = false;
            for path in event.paths {
                let relative = match path.strip_prefix(repo_root) {
                    Ok(rel) => normalize_rel_path(rel),
                    Err(_) => continue,
                };
                if relative.is_empty() {
                    continue;
                }
                saw_repo_relative_path = true;
                if should_skip_path(&relative) {
                    continue;
                }
                pending.changed_paths.insert(relative);
                added_tracked_path = true;
            }
            if !added_tracked_path && !saw_repo_relative_path {
                emit_dirty_event(
                    pending,
                    debounce_ms,
                    format!("opaque watch event {:?}; reconciling", event.kind),
                    emit,
                )?;
            }
            Ok(())
        }
        Err(error) => emit_dirty_event(pending, debounce_ms, describe_watch_error(&error), emit),
    }
}

fn emit_dirty_event<F>(
    pending: &mut PendingWatchState,
    debounce_ms: u64,
    note: String,
    emit: &mut F,
) -> Result<(), CodeGraphError>
where
    F: FnMut(GraphWatchEvent) -> Result<(), CodeGraphError>,
{
    if !pending.dirty_notes.insert(note.clone()) {
        return Ok(());
    }
    emit(GraphWatchEvent {
        payload: GraphWatchEventPayload {
            kind: "dirty".to_owned(),
            debounce_ms,
            changed_paths: pending.changed_paths.iter().cloned().collect(),
            dirty: true,
            refresh_duration_ms: None,
            index: None,
            notes: vec![note],
        },
    })
}

fn flush_watch_batch(
    repo_root: &Path,
    debounce_ms: u64,
    pending: &mut PendingWatchState,
) -> Result<GraphWatchEventPayload, CodeGraphError> {
    let observed_changed_paths = pending.changed_paths.iter().cloned().collect::<Vec<_>>();
    let dirty_notes = pending.dirty_notes.iter().cloned().collect::<Vec<_>>();
    let dirty = !dirty_notes.is_empty();
    pending.changed_paths.clear();
    pending.dirty_notes.clear();

    let started = Instant::now();
    let _refresh_lock = crate::refresh::RefreshLock::acquire_wait(repo_root, WATCH_LOCK_WAIT_MS)?;
    let report = run_index(repo_root)?;
    let changed_paths = merged_changed_paths(&observed_changed_paths, &report.changed_paths);
    Ok(GraphWatchEventPayload {
        kind: if dirty {
            "reconcile".to_owned()
        } else {
            "refresh".to_owned()
        },
        debounce_ms,
        changed_paths,
        dirty,
        refresh_duration_ms: Some(started.elapsed().as_millis()),
        index: Some(GraphIndexPayload {
            indexed_files: report.indexed_files,
            extractor_count: report.extractor_count,
            counts: report.counts,
            stale_paths: report.stale_paths,
            new_paths: report.new_paths,
            changed_paths: report.changed_paths,
            deleted_paths: report.deleted_paths,
            skipped_paths: report.skipped_paths,
            failed_paths: report.failed_paths,
        }),
        notes: dirty_notes,
    })
}

fn merged_changed_paths(observed_paths: &[String], indexed_paths: &[String]) -> Vec<String> {
    let mut merged = BTreeSet::new();
    merged.extend(observed_paths.iter().cloned());
    merged.extend(indexed_paths.iter().cloned());
    merged.into_iter().collect()
}

fn should_trigger_refresh(kind: &EventKind) -> bool {
    !matches!(kind, EventKind::Access(_))
}

/// How long a watch batch waits for an in-flight refresh before proceeding
/// without the lock (batches are idempotent and SQLite serializes writers).
const WATCH_LOCK_WAIT_MS: u64 = 10_000;

fn describe_watch_error(error: &notify::Error) -> String {
    match &error.kind {
        notify::ErrorKind::MaxFilesWatch => format!("watch backend overflow: {error}"),
        _ => format!("watch backend error: {error}"),
    }
}

impl PendingWatchState {
    fn is_empty(&self) -> bool {
        self.changed_paths.is_empty() && self.dirty_notes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn collect_watch_event_marks_dirty_on_backend_error() {
        let root = tempfile::tempdir().expect("tempdir");
        let mut pending = PendingWatchState::default();
        let mut emitted = Vec::<GraphWatchEventPayload>::new();
        let error = notify::Error::new(notify::ErrorKind::MaxFilesWatch);

        collect_watch_event(root.path(), Err(error), 1000, &mut pending, &mut |event| {
            emitted.push(event.payload);
            Ok(())
        })
        .expect("collect");

        assert!(pending
            .dirty_notes
            .iter()
            .any(|note| note.contains("overflow")));
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].kind, "dirty");
        assert!(emitted[0].dirty);
    }

    #[test]
    fn flush_watch_batch_reconciles_deleted_files_when_dirty() {
        let root = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(root.path().join("src")).expect("mkdir src");
        fs::write(
            root.path().join("effigy.toml"),
            "[tasks.build]\nrun = \"echo ok\"\n",
        )
        .expect("manifest");
        fs::write(root.path().join("src/lib.rs"), "pub fn alpha() {}\n").expect("rust");

        run_index(root.path()).expect("initial index");
        fs::remove_file(root.path().join("src/lib.rs")).expect("remove rust");

        let mut pending = PendingWatchState::default();
        pending
            .dirty_notes
            .insert("watch backend error: synthetic".to_owned());

        let payload = flush_watch_batch(root.path(), 1000, &mut pending).expect("flush");

        assert_eq!(payload.kind, "reconcile");
        assert!(payload.dirty);
        assert!(payload
            .index
            .as_ref()
            .is_some_and(|index| index.deleted_paths.iter().any(|path| path == "src/lib.rs")));
        assert!(pending.is_empty());
    }

    #[test]
    fn merged_changed_paths_unions_observed_and_indexed_paths() {
        let merged = merged_changed_paths(
            &["src".to_owned(), "src/lib.rs".to_owned()],
            &["src/lib.rs".to_owned(), "src/other.rs".to_owned()],
        );

        assert_eq!(
            merged,
            vec![
                "src".to_owned(),
                "src/lib.rs".to_owned(),
                "src/other.rs".to_owned()
            ]
        );
    }
}
