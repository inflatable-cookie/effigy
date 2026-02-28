use std::path::PathBuf;

use super::manifest::{ManifestTask, TaskManifest};

#[derive(Debug)]
pub(super) struct LoadedCatalog {
    pub(super) alias: String,
    pub(super) catalog_root: PathBuf,
    pub(super) manifest_path: PathBuf,
    pub(super) manifest: TaskManifest,
    pub(super) defer_run: Option<String>,
    pub(super) depth: usize,
}

#[derive(Debug)]
pub(super) struct TaskSelector {
    pub(super) prefix: Option<String>,
    pub(super) task_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CatalogSelectionMode {
    ExplicitPrefix,
    CwdNearest,
    RootShallowest,
}

#[derive(Debug)]
pub(super) struct TaskSelection<'a> {
    pub(super) catalog: &'a LoadedCatalog,
    pub(super) task: &'a ManifestTask,
    pub(super) mode: CatalogSelectionMode,
    pub(super) evidence: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct DeferredCommand {
    pub(super) template: String,
    pub(super) working_dir: PathBuf,
    pub(super) source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TaskRuntimeArgs {
    pub(super) repo_override: Option<PathBuf>,
    pub(super) verbose_root: bool,
    pub(super) passthrough: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct ManagedProcessSpec {
    pub(super) name: String,
    pub(super) run: String,
    pub(super) cwd: PathBuf,
    pub(super) start_after_ms: u64,
}

#[derive(Debug)]
pub(super) struct ManagedTaskPlan {
    pub(super) mode: String,
    pub(super) profile: String,
    pub(super) processes: Vec<ManagedProcessSpec>,
    pub(super) tab_order: Vec<String>,
    pub(super) fail_on_non_zero: bool,
    pub(super) passthrough: Vec<String>,
}

pub(super) const TASK_MANIFEST_FILE: &str = "effigy.toml";
pub(super) const DEFER_DEPTH_ENV: &str = "EFFIGY_DEFER_DEPTH";
pub(super) const IMPLICIT_ROOT_DEFER_TEMPLATE: &str =
    "composer global exec effigy -- {request} {args}";
pub(super) const DEFAULT_BUILTIN_TEST_MAX_PARALLEL: usize = 3;
pub(super) const DEFAULT_MANAGED_SHELL_RUN: &str = "exec ${SHELL:-/bin/zsh} -i";
pub(super) const BUILTIN_TASKS: [(&str, &str); 5] = [
    ("help", "Show general help (same as --help)"),
    (
        "config",
        "Show supported project effigy.toml configuration keys and examples",
    ),
    (
        "doctor",
        "Built-in remedial health checks for environment, manifests, and task references",
    ),
    (
        "test",
        "Built-in test runner detection, supports <catalog>/test fallback, optional --plan",
    ),
    ("tasks", "List discovered catalogs and available tasks"),
];
