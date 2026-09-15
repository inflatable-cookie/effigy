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

/// Product lifecycle surface a task definition belongs to.
///
/// `[tasks]` definitions are published; `[drafts]` definitions are
/// provisional. The discriminator is carried through selection, execution
/// requests, and status identity so the two surfaces cannot collide. It is
/// never an access-control or secrecy boundary.
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
