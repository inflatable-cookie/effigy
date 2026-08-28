mod error;
mod listing;
mod parsing;
mod probe;
mod reference;
pub mod testing;
mod view;

use std::path::PathBuf;

pub use effigy_core::resolver::ResolutionMode;
pub use effigy_core::task_selection::{CatalogSelectionMode, TaskSelector};
pub use error::EffigyTasksError;
pub use listing::{
    list_tasks, render_task_listing_json, render_task_listing_text, ListTasksRequest,
    ListTasksResult,
};
pub use parsing::{
    normalize_builtin_test_suite, parse_task_runtime_args, parse_task_selector,
    render_task_selector,
};
pub use probe::{probe_task_resolution, ProbeTaskResolutionRequest, TaskResolutionProbe};
pub use reference::{
    command_passthrough_args, parse_task_reference_invocation, render_passthrough_args,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskContext {
    pub target_repo: PathBuf,
    pub cwd: PathBuf,
    pub resolution_mode: ResolutionMode,
    pub resolution_evidence: Vec<String>,
    pub resolution_warnings: Vec<String>,
}

pub trait Task {
    type Collected;
    type Evaluated;

    fn id(&self) -> &'static str;
    fn collect(&self, ctx: &TaskContext) -> Result<Self::Collected, TaskError>;
    fn evaluate(&self, collected: Self::Collected) -> Result<Self::Evaluated, TaskError>;
    fn render(&self, evaluated: Self::Evaluated) -> Result<String, TaskError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskError {
    Io(String),
}

impl std::fmt::Display for TaskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskError::Io(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for TaskError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRuntimeArgs {
    pub repo_override: Option<PathBuf>,
    pub verbose_root: bool,
    pub env_schema_override: Option<PathBuf>,
    pub passthrough: Vec<String>,
}
