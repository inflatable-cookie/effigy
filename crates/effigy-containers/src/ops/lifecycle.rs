use crate::{ContainerConfirmationPolicy, ContainerSideEffectClass};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerLifecycleOperation {
    Up(ContainerUpOperation),
    Down(ContainerDownOperation),
    Reset(ContainerResetOperation),
}

impl ContainerLifecycleOperation {
    pub fn up(attach: bool, detach: bool) -> Self {
        Self::Up(ContainerUpOperation { attach, detach })
    }

    pub fn down(all: bool) -> Self {
        Self::Down(ContainerDownOperation { all })
    }

    pub fn reset(keep_data: bool, wipe_data: bool, assume_yes: bool) -> Self {
        Self::Reset(ContainerResetOperation {
            keep_data,
            wipe_data,
            assume_yes,
        })
    }

    pub fn side_effect_class(&self) -> ContainerSideEffectClass {
        match self {
            Self::Up(_) => ContainerSideEffectClass::StartsRuntime,
            Self::Down(_) => ContainerSideEffectClass::StopsRuntime,
            Self::Reset(operation) if operation.wipe_data => {
                ContainerSideEffectClass::DestroysRuntimeData
            }
            Self::Reset(_) => ContainerSideEffectClass::RecreatesRuntime,
        }
    }

    pub fn confirmation_policy(&self) -> ContainerConfirmationPolicy {
        match self {
            Self::Reset(operation) if operation.wipe_data && !operation.assume_yes => {
                ContainerConfirmationPolicy::RequireConfirmation {
                    reason: "reset removes runtime data",
                }
            }
            _ => ContainerConfirmationPolicy::NoConfirmationRequired,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContainerUpOperation {
    pub attach: bool,
    pub detach: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContainerDownOperation {
    pub all: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContainerResetOperation {
    pub keep_data: bool,
    pub wipe_data: bool,
    pub assume_yes: bool,
}
