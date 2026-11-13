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
use std::path::PathBuf;

use effigy_core::task_selection::{CatalogSelectionMode, TaskSelector};

use crate::task_runtime::{ManifestTask, ManifestTaskRunIn};
use crate::TaskManifest;

/// Callback signature for resolving a `TaskSelector` against a slice
/// of `LoadedCatalog`. The runner owns the routing implementation
/// (currently `effigy_routing::select_catalog_and_task`); managed task
/// orchestration takes this as a parameter rather than importing the routing
/// core directly so it stays extract-ready.
///
/// The error channel is [`TaskResolveError`], so a caller decides by failure
/// class instead of re-reading text.
pub type TaskResolverFn<'r> = &'r dyn for<'c> Fn(
    &TaskSelector,
    &'c [LoadedCatalog],
    &std::path::Path,
) -> Result<TaskSelection<'c>, TaskResolveError>;

/// Why a resolver could not produce a `TaskSelection`. Routing owns the detailed
/// `RoutingError`; this type carries the class across the manifest/managed
/// boundary. Only [`TaskResolveError::NotFound`] means "no catalog declares this
/// task", so a builtin fallback must not fire for an unreadable catalog graph or
/// an ambiguous selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskResolveError {
    /// No catalog declares the task, or the prefix names no catalog.
    NotFound { detail: String },
    /// More than one catalog could own the task and the invocation cwd did not
    /// choose between them.
    Ambiguous { detail: String },
    /// The catalog graph or a manifest could not be read or parsed.
    Unavailable { detail: String },
}

impl TaskResolveError {
    pub fn not_found(detail: impl Into<String>) -> Self {
        Self::NotFound {
            detail: detail.into(),
        }
    }

    pub fn ambiguous(detail: impl Into<String>) -> Self {
        Self::Ambiguous {
            detail: detail.into(),
        }
    }

    pub fn unavailable(detail: impl Into<String>) -> Self {
        Self::Unavailable {
            detail: detail.into(),
        }
    }

    /// True only when no catalog declares the task. This is the one class a
    /// caller may treat as "not a catalog task".
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound { .. })
    }
}

impl std::fmt::Display for TaskResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound { detail }
            | Self::Ambiguous { detail }
            | Self::Unavailable { detail } => f.write_str(detail),
        }
    }
}

impl std::error::Error for TaskResolveError {}

#[derive(Debug)]
pub struct LoadedCatalog {
    pub alias: String,
    pub catalog_root: PathBuf,
    pub manifest_path: PathBuf,
    pub bundle_root: Option<PathBuf>,
    pub manifest: TaskManifest,
    /// Digest of the effective composed value used to parse this catalog.
    /// Synthetic catalogs and readers without composition evidence use None.
    pub effective_digest: Option<effigy_kernel::id::Digest>,
    pub defer_run: Option<String>,
    pub deferred_builtins: BTreeSet<String>,
    pub depth: usize,
}

impl LoadedCatalog {
    /// Build a catalog fixture for tests.
    ///
    /// Everything a test does not vary is derived here, so a new
    /// `LoadedCatalog` field never again edits every fixture literal across
    /// the workspace: the manifest path is the catalog root's own manifest,
    /// and bundle, deferral, and deferred-builtin state stay empty.
    pub fn for_test(
        alias: impl Into<String>,
        catalog_root: impl Into<PathBuf>,
        manifest: TaskManifest,
        depth: usize,
    ) -> Self {
        let catalog_root = catalog_root.into();
        Self {
            alias: alias.into(),
            manifest_path: catalog_root.join(crate::TASK_MANIFEST_FILE),
            catalog_root,
            bundle_root: None,
            manifest,
            effective_digest: None,
            defer_run: None,
            deferred_builtins: BTreeSet::new(),
            depth,
        }
    }
}

#[derive(Debug)]
pub struct TaskSelection<'a> {
    pub catalog: &'a LoadedCatalog,
    pub task: &'a ManifestTask,
    pub mode: CatalogSelectionMode,
    pub evidence: Vec<String>,
    /// Published or draft surface the selection resolved in.
    pub surface: effigy_core::task_selection::TaskSurface,
}

#[derive(Debug, Clone)]
pub struct DeferredCommand {
    pub template: String,
    pub working_dir: PathBuf,
    pub source: String,
    pub run_in: ManifestTaskRunIn,
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn a_retired_catalog_graph_table_is_ignored_with_its_replacement() {
        for fields in [
            "",
            "segmented = true",
            "segmented = true\nindependent = true",
            "independent = true",
            "store = 'custom.db'",
        ] {
            let body = format!(
                "[catalog]\nalias = 'bovine'\n[catalog.graph]\n{fields}\n\n[tasks.build]\nrun = 'true'\n"
            );
            let manifest = toml::from_str::<TaskManifest>(&body).expect("retired table loads");
            assert_eq!(
                manifest
                    .catalog
                    .as_ref()
                    .and_then(|catalog| catalog.alias.clone()),
                Some("bovine".to_owned())
            );
            let notice = manifest
                .retired_config
                .notice(&PathBuf::from("effigy.toml"))
                .expect("one notice");
            assert!(notice.contains("[catalog.graph]"), "{notice}");
            assert!(notice.contains("no replacement"), "{notice}");
        }
    }

    /// Only "no catalog declares this task" is a fallback-eligible class; an
    /// unreadable graph and an ambiguous selection stay distinct so a caller
    /// cannot mistake them for a missing task.
    #[test]
    fn resolver_failure_classes_stay_distinct() {
        let not_found = TaskResolveError::not_found("no catalog declares `build`");
        assert!(not_found.is_not_found());
        assert_eq!(not_found.to_string(), "no catalog declares `build`");

        for error in [
            TaskResolveError::ambiguous("`build` is declared by `a` and `b`"),
            TaskResolveError::unavailable("the manifest could not be read"),
        ] {
            assert!(!error.is_not_found(), "{error:?}");
            assert!(!error.to_string().is_empty());
        }
    }
}
