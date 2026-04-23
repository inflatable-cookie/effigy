//! Loaded-catalog shapes used by task-runtime consumers.
//!
//! `LoadedCatalog` bundles a parsed `TaskManifest` with the metadata a
//! runtime needs to locate and interpret it: alias, catalog root, manifest
//! path, defer spec, and composition depth. `TaskSelection` names a
//! chosen task inside a loaded catalog; `DeferredCommand` carries the
//! shell fallback template when a selector doesn't resolve locally.
//!
//! These types previously lived inside the runner binary
//! (`src/runner/model/catalog.rs`). They moved here so the
//! `effigy-managed` extraction can depend on them without the runner
//! having to expose internal module paths.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use effigy_tasks::{CatalogSelectionMode, TaskSelector};

use crate::task_runtime::ManifestTask;
use crate::TaskManifest;

/// Callback signature for resolving a `TaskSelector` against a slice
/// of `LoadedCatalog`. The runner owns the routing implementation
/// (currently `src/runner/catalog::select_catalog_and_task`); managed
/// task orchestration takes this as a parameter rather than importing
/// the routing core directly so it stays extract-ready.
///
/// The error channel is plain `String` — both runners and managed
/// orchestration have their own error enums with `task_invocation`
/// constructors that accept strings, so callers wrap at the boundary.
pub type TaskResolverFn<'r> = &'r dyn for<'c> Fn(
    &TaskSelector,
    &'c [LoadedCatalog],
    &Path,
) -> Result<TaskSelection<'c>, String>;

#[derive(Debug)]
pub struct LoadedCatalog {
    pub alias: String,
    pub catalog_root: PathBuf,
    pub manifest_path: PathBuf,
    pub bundle_root: Option<PathBuf>,
    pub manifest: TaskManifest,
    pub defer_run: Option<String>,
    pub deferred_builtins: BTreeSet<String>,
    pub depth: usize,
}

#[derive(Debug)]
pub struct TaskSelection<'a> {
    pub catalog: &'a LoadedCatalog,
    pub task: &'a ManifestTask,
    pub mode: CatalogSelectionMode,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DeferredCommand {
    pub template: String,
    pub working_dir: PathBuf,
    pub source: String,
}
