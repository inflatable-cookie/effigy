#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SetupCategory {
    Baseline,
    Tasks,
    Health,
    Graph,
    Secrets,
    Runtime,
    Bundles,
    Validation,
    Advanced,
}

impl SetupCategory {
    pub(crate) fn heading(self) -> &'static str {
        match self {
            Self::Baseline => "Baseline",
            Self::Tasks => "Task adoption",
            Self::Health => "Repo health",
            Self::Graph => "Graph",
            Self::Secrets => "Secrets",
            Self::Runtime => "Runtime",
            Self::Bundles => "Bundles",
            Self::Validation => "Validation",
            Self::Advanced => "Advanced surfaces",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SetupExecutionKind {
    Apply,
    Inspect,
    Guidance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SetupSafetyClass {
    SafeCheck,
    SafeApply,
    ContextualApply,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SetupApplicability {
    Applicable,
    AlreadySatisfied,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SetupJob {
    pub(crate) id: String,
    pub(crate) category: SetupCategory,
    pub(crate) execution_kind: SetupExecutionKind,
    pub(crate) safety_class: SetupSafetyClass,
    pub(crate) applicability: SetupApplicability,
    pub(crate) summary: String,
    pub(crate) reason: String,
    pub(crate) recommended_command: Option<String>,
    pub(crate) can_run_noninteractive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SetupActionStatus {
    Applied,
    Inspected,
    Skipped,
    Guided,
    Blocked,
    Failed,
}

impl SetupActionStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Inspected => "inspected",
            Self::Skipped => "skipped",
            Self::Guided => "guided",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SetupActionOutcome {
    pub(crate) id: String,
    pub(crate) status: SetupActionStatus,
    pub(crate) summary: String,
    pub(crate) reason: String,
    pub(crate) command: Option<String>,
    pub(crate) output: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct InitActionReport {
    pub(crate) selected_action_ids: Vec<String>,
    pub(crate) outcomes: Vec<SetupActionOutcome>,
}
