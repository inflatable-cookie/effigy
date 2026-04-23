#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSelector {
    pub prefix: Option<String>,
    pub task_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogSelectionMode {
    ExplicitPrefix,
    CwdNearest,
    RootShallowest,
}
