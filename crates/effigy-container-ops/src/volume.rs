use crate::{ContainerConfirmationPolicy, ContainerSideEffectClass};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerVolumeOperation {
    List(ContainerVolumeListOperation),
    Prune(ContainerVolumePruneOperation),
}

impl ContainerVolumeOperation {
    pub fn list(orphans_only: bool, profile: Option<String>) -> Self {
        Self::List(ContainerVolumeListOperation {
            orphans_only,
            profile,
        })
    }

    pub fn prune(orphans_only: bool, profile: Option<String>) -> Self {
        Self::Prune(ContainerVolumePruneOperation {
            orphans_only,
            profile,
        })
    }

    pub fn side_effect_class(&self) -> ContainerSideEffectClass {
        match self {
            Self::List(_) => ContainerSideEffectClass::ReadsRuntime,
            Self::Prune(_) => ContainerSideEffectClass::MutatesRuntimeData,
        }
    }

    pub fn confirmation_policy(&self) -> ContainerConfirmationPolicy {
        match self {
            Self::List(_) => ContainerConfirmationPolicy::NoConfirmationRequired,
            Self::Prune(_) => ContainerConfirmationPolicy::RequireConfirmation {
                reason: "volume prune deletes named runtime storage",
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerVolumeListOperation {
    pub orphans_only: bool,
    pub profile: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerVolumePruneOperation {
    pub orphans_only: bool,
    pub profile: Option<String>,
}
