use std::path::PathBuf;

use super::super::manifest::task_runtime::ManifestTask;
use super::super::manifest::TaskManifest;

#[derive(Debug)]
pub(in crate::runner) struct LoadedCatalog {
    pub(in crate::runner) alias: String,
    pub(in crate::runner) catalog_root: PathBuf,
    pub(in crate::runner) manifest_path: PathBuf,
    pub(in crate::runner) manifest: TaskManifest,
    pub(in crate::runner) defer_run: Option<String>,
    pub(in crate::runner) depth: usize,
}

#[derive(Debug)]
pub(in crate::runner) struct TaskSelector {
    pub(in crate::runner) prefix: Option<String>,
    pub(in crate::runner) task_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) enum CatalogSelectionMode {
    ExplicitPrefix,
    CwdNearest,
    RootShallowest,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct TaskRuntimeArgs {
    pub(in crate::runner) repo_override: Option<PathBuf>,
    pub(in crate::runner) verbose_root: bool,
    pub(in crate::runner) env_schema_override: Option<PathBuf>,
    pub(in crate::runner) passthrough: Vec<String>,
}
