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

/// Persisted task identity discriminator.
///
/// Every current task uses Published, independently of public/internal
/// discovery visibility. Draft remains decodable for historical records;
/// it no longer names a selectable task namespace.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Default,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum TaskSurface {
    #[default]
    Published,
    Draft,
}

impl TaskSurface {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Published => "published",
            Self::Draft => "draft",
        }
    }
}

impl std::fmt::Display for TaskSurface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
