use crate::{ContainerConfirmationPolicy, ContainerSideEffectClass};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerCacheOperation {
    List(ContainerCacheListOperation),
    Prune(ContainerCachePruneOperation),
}

impl ContainerCacheOperation {
    pub fn list(all: bool, project: Option<String>, kind: Option<String>) -> Self {
        Self::List(ContainerCacheListOperation { all, project, kind })
    }

    pub fn prune(
        all: bool,
        project: Option<String>,
        kind: Option<String>,
        assume_yes: bool,
    ) -> Self {
        Self::Prune(ContainerCachePruneOperation {
            all,
            project,
            kind,
            assume_yes,
        })
    }

    pub fn side_effect_class(&self) -> ContainerSideEffectClass {
        match self {
            Self::List(_) => ContainerSideEffectClass::ReadsRuntime,
            Self::Prune(_) => ContainerSideEffectClass::RemovesCacheData,
        }
    }

    pub fn confirmation_policy(&self) -> ContainerConfirmationPolicy {
        match self {
            Self::Prune(operation) if !operation.assume_yes => {
                ContainerConfirmationPolicy::RequireConfirmation {
                    reason: "operation removes cache data",
                }
            }
            _ => ContainerConfirmationPolicy::NoConfirmationRequired,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerCacheListOperation {
    pub all: bool,
    pub project: Option<String>,
    pub kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerCachePruneOperation {
    pub all: bool,
    pub project: Option<String>,
    pub kind: Option<String>,
    pub assume_yes: bool,
}
