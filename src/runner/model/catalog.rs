use std::collections::BTreeSet;
use std::path::PathBuf;

pub(in crate::runner) use effigy_tasks::{CatalogSelectionMode, TaskRuntimeArgs, TaskSelector};

use super::super::manifest::task_runtime::ManifestTask;
use super::super::manifest::TaskManifest;

#[derive(Debug)]
pub(in crate::runner) struct LoadedCatalog {
    pub(in crate::runner) alias: String,
    pub(in crate::runner) catalog_root: PathBuf,
    pub(in crate::runner) manifest_path: PathBuf,
    pub(in crate::runner) manifest: TaskManifest,
    pub(in crate::runner) defer_run: Option<String>,
    pub(in crate::runner) deferred_builtins: BTreeSet<String>,
    pub(in crate::runner) depth: usize,
}

#[derive(Debug)]
pub(in crate::runner) struct TaskSelection<'a> {
    pub(in crate::runner) catalog: &'a LoadedCatalog,
    pub(in crate::runner) task: &'a ManifestTask,
    pub(in crate::runner) mode: CatalogSelectionMode,
    pub(in crate::runner) evidence: Vec<String>,
}

#[derive(Debug, Clone)]
pub(in crate::runner) struct DeferredCommand {
    pub(in crate::runner) template: String,
    pub(in crate::runner) working_dir: PathBuf,
    pub(in crate::runner) source: String,
}
