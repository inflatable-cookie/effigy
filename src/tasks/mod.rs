pub mod pulse;

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskContext {
    pub target_repo: PathBuf,
    pub cwd: PathBuf,
    pub resolution_mode: ResolutionMode,
    pub resolution_evidence: Vec<String>,
    pub resolution_warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionMode {
    Explicit,
    AutoNearest,
    AutoPromoted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PulseReport {
    pub repo: String,
    pub evidence: Vec<String>,
    pub risk: Vec<String>,
    pub next_action: Vec<String>,
    pub owner: String,
    pub eta: String,
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
